use crate::cartridge_io::CartridgeIo;
use crate::core::cpu_regs::InterruptFlag;
use crate::core::cycle_manager::{CycleManager, EventType};
use crate::core::emu::{get_cm_mut, get_common_mut, get_cpu_regs_mut, get_mem_mut, io_dma, Emu};
use crate::core::memory::dma::DmaTransferMode;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::logging::debug_println;
use crate::mmap::PAGE_SIZE;
use crate::utils;
use bilge::prelude::*;
use std::ptr;

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
struct AuxSpiCnt {
    baudrate: u2,
    not_used: u4,
    hold_chipselect: bool,
    busy: u1,
    not_used2: u5,
    nds_slot_mode: u1,
    transfer_ready_irq: bool,
    nds_slot_enable: u1,
}

impl Default for AuxSpiCnt {
    fn default() -> Self {
        AuxSpiCnt::from(0)
    }
}

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
pub struct RomCtrl {
    key1_gap_length: u13,
    key2_encrypt_data: u1,
    se: u1,
    key2_apply_seed: u1,
    key1_gap2_length: u6,
    key2_encrypt_cmd: u1,
    data_word_status: bool,
    data_block_size: u3,
    transfer_clk_rate: u1,
    key1_gap_clks: u1,
    resb_release_reset: u1,
    wr: u1,
    block_start_status: bool,
}

impl Default for RomCtrl {
    fn default() -> Self {
        RomCtrl::from(0)
    }
}

#[derive(Debug)]
enum CmdMode {
    Header,
    Chip,
    Secure,
    Data,
    None,
}

#[derive(Default)]
struct CartridgeInner {
    block_size: u16,
    read_count: u16,
    read_addr: u32,
    encrypted: bool,

    aux_command: u8,
    aux_address: u32,
    aux_write_count: u32,
    aux_spi_hold: bool,

    aux_spi_cnt: AuxSpiCnt,
    aux_spi_data: u8,
    rom_ctrl: RomCtrl,
    bus_cmd_out: u64,
}

pub struct Cartridge {
    pub io: CartridgeIo,
    cmd_mode: CmdMode,
    inner: [CartridgeInner; 2],
    read_buf: *const [u8; PAGE_SIZE],
    read_buf_addr: u32,
}

impl Cartridge {
    pub fn new(cartridge_io: CartridgeIo) -> Self {
        Cartridge {
            io: cartridge_io,
            inner: [CartridgeInner::default(), CartridgeInner::default()],
            cmd_mode: CmdMode::None,
            read_buf: ptr::null(),
            read_buf_addr: u32::MAX,
        }
    }

    fn set_read_buf_addr(&mut self, addr: u32) {
        let page_addr = addr & !(PAGE_SIZE as u32 - 1);
        if page_addr != self.read_buf_addr {
            self.read_buf = self.io.get_page(page_addr).unwrap();
            self.read_buf_addr = page_addr;
        }
    }

    fn read_buf(&self, addr: u32) -> u32 {
        let buf = unsafe { self.read_buf.as_ref_unchecked() };
        utils::read_from_mem(buf, addr & (PAGE_SIZE as u32 - 1))
    }

    pub fn get_aux_spi_cnt(&self, cpu: CpuType) -> u16 {
        u16::from(self.inner[cpu].aux_spi_cnt)
    }

    pub fn get_aux_spi_data(&self, cpu: CpuType) -> u8 {
        self.inner[cpu].aux_spi_data
    }

    pub fn get_rom_ctrl(&self, cpu: CpuType) -> u32 {
        self.inner[cpu].rom_ctrl.into()
    }

    pub fn get_rom_data_in(&mut self, cpu: CpuType, emu: &mut Emu) -> u32 {
        let inner = &mut self.inner[cpu];
        if !inner.rom_ctrl.data_word_status() || !inner.rom_ctrl.block_start_status() {
            return 0;
        }

        inner.read_count += 4;
        if inner.read_count == inner.block_size {
            inner.rom_ctrl.set_data_word_status(false);
            inner.rom_ctrl.set_block_start_status(false);
            if inner.aux_spi_cnt.transfer_ready_irq() {
                get_cpu_regs_mut!(emu, cpu).send_interrupt(InterruptFlag::NdsSlotTransferCompletion, emu);
            }
        } else {
            get_cm_mut!(emu).schedule_imm(
                match cpu {
                    ARM9 => EventType::CartridgeWordReadArm9,
                    ARM7 => EventType::CartridgeWordReadArm7,
                },
                0,
            );
        }

        let ret = match self.cmd_mode {
            CmdMode::Header => {
                let read_count = (inner.read_count - 4) & 0xFFF;
                let read_addr = read_count as u32 + inner.read_addr;
                self.set_read_buf_addr(read_addr);
                self.read_buf(read_addr)
            }
            CmdMode::Chip => 0x00001FC2,
            CmdMode::Secure => {
                todo!()
            }
            CmdMode::Data => {
                if inner.read_count - 4 < inner.block_size {
                    let read_addr = inner.read_count as u32 - 4 + inner.read_addr;
                    self.set_read_buf_addr(read_addr);
                    self.read_buf(read_addr)
                } else {
                    0xFFFFFFFF
                }
            }
            CmdMode::None => 0xFFFFFFFF,
        };

        ret
    }

    pub fn set_aux_spi_cnt(&mut self, cpu: CpuType, mut mask: u16, value: u16) {
        mask &= 0xE043;
        self.inner[cpu].aux_spi_cnt = ((u16::from(self.inner[cpu].aux_spi_cnt) & !mask) | (value & mask)).into();
    }

    pub fn set_aux_spi_data(&mut self, cpu: CpuType, value: u8) {
        let inner = &mut self.inner[cpu];

        if inner.aux_write_count == 0 {
            match value {
                0x4 | 0x6 => inner.aux_spi_data = 0,
                _ => {
                    inner.aux_command = value;
                    inner.aux_address = 0;
                    inner.aux_spi_data = 0xFF;
                }
            }
        } else {
            if self.io.save_file_size == 0 {
                match inner.aux_command {
                    0x0B => {
                        self.io.resize_save_file(0x200);
                        debug_println!("Detected EEPROM 0.5KB save type");
                    }
                    0x02 => {
                        self.io.resize_save_file(0x10000);
                        debug_println!("Detected EEPROM 64KB save type");
                    }
                    0x0A => {
                        self.io.resize_save_file(0x80000);
                        debug_println!("Detected FLASH 512KB save type");
                    }
                    _ => {}
                }
            }

            let save_size = self.io.save_file_size;
            match save_size {
                0x200 => match inner.aux_command {
                    0x03 | 0x0B => {
                        if inner.aux_write_count < 2 {
                            inner.aux_address = value as u32;
                            inner.aux_spi_data = 0;
                        } else {
                            let addr_offset = if inner.aux_command == 0x0B { 0x100 } else { 0 };
                            inner.aux_spi_data = self.io.read_save_buf((inner.aux_address + addr_offset) & (save_size - 1));
                            inner.aux_address += 1;
                        }
                    }
                    0x02 | 0x0A => {
                        if inner.aux_write_count < 2 {
                            inner.aux_address = value as u32;
                            inner.aux_spi_data = 0;
                        } else {
                            let addr_offset = if inner.aux_command == 0x0A { 0x100 } else { 0 };
                            self.io.write_save_buf((inner.aux_address + addr_offset) & (save_size - 1), value);
                            inner.aux_address += 1;
                            inner.aux_spi_data = 0;
                        }
                    }
                    0x01 | 0x05 => inner.aux_spi_data = 0,
                    _ => {
                        debug_println!("Unknown EEPROM 0.5KB command {:x}", inner.aux_command);
                        inner.aux_spi_data = 0xFF;
                    }
                },
                0x2000 | 0x8000 | 0x10000 | 0x20000 => match inner.aux_command {
                    0x03 => {
                        if inner.aux_write_count < if save_size == 0x20000 { 4 } else { 3 } {
                            inner.aux_address |= (value as u32) << ((if save_size == 0x20000 { 3 } else { 2 } - inner.aux_write_count) * 8);
                            inner.aux_spi_data = 0;
                        } else {
                            inner.aux_spi_data = if inner.aux_address < save_size { self.io.read_save_buf(inner.aux_address) } else { 0 };
                            inner.aux_address += 1;
                        }
                    }
                    0x02 => {
                        if inner.aux_write_count < if save_size == 0x20000 { 4 } else { 3 } {
                            inner.aux_address |= (value as u32) << ((if save_size == 0x20000 { 3 } else { 2 } - inner.aux_write_count) * 8);
                            inner.aux_spi_data = 0;
                        } else {
                            if inner.aux_address < save_size {
                                self.io.write_save_buf(inner.aux_address, value);
                            }
                            inner.aux_address += 1;
                            inner.aux_spi_data = 0;
                        }
                    }
                    _ => {
                        debug_println!("Unknown EEPROM/FRAM command {:x}", inner.aux_command);
                        inner.aux_spi_data = 0;
                    }
                },
                0x40000 | 0x80000 | 0x100000 | 0x800000 => match inner.aux_command {
                    0x03 => {
                        if inner.aux_write_count < 4 {
                            inner.aux_address |= (value as u32) << ((3 - inner.aux_write_count) * 8);
                            inner.aux_spi_data = 0;
                        } else {
                            inner.aux_spi_data = if inner.aux_address < save_size { self.io.read_save_buf(inner.aux_address) } else { 0 };
                            inner.aux_address += 1;
                        }
                    }
                    0x0A => {
                        if inner.aux_write_count < 4 {
                            inner.aux_address |= (value as u32) << ((3 - inner.aux_write_count) * 8);
                            inner.aux_spi_data = 0;
                        } else {
                            if inner.aux_address < save_size {
                                self.io.write_save_buf(inner.aux_address, value);
                            }
                            inner.aux_address += 1;
                            inner.aux_spi_data = 0;
                        }
                    }
                    0x08 => inner.aux_spi_data = if self.io.header.game_code[0] == b'I' { 0xAA } else { 0 },
                    _ => {
                        debug_println!("Unknown FLASH command {:x}", inner.aux_command);
                        inner.aux_spi_data = 0;
                    }
                },
                _ => {
                    debug_println!("Unknown api size {:x}", self.io.save_file_size)
                }
            }
        }

        if inner.aux_spi_cnt.hold_chipselect() {
            inner.aux_write_count += 1;
        } else {
            inner.aux_write_count = 0;
        }
    }

    pub fn set_bus_cmd_out_l(&mut self, cpu: CpuType, mask: u32, value: u32) {
        self.inner[cpu].bus_cmd_out = (self.inner[cpu].bus_cmd_out & !(mask as u64)) | (value & mask) as u64;
    }

    pub fn set_bus_cmd_out_h(&mut self, cpu: CpuType, mask: u32, value: u32) {
        self.inner[cpu].bus_cmd_out = (self.inner[cpu].bus_cmd_out & !((mask as u64) << 32)) | ((value & mask) as u64) << 32;
    }

    pub fn set_rom_ctrl(&mut self, cpu: CpuType, mut mask: u32, value: u32, emu: &mut Emu) {
        let new_rom_ctrl = RomCtrl::from(value);
        let inner = &mut self.inner[cpu];

        inner.rom_ctrl.set_resb_release_reset(new_rom_ctrl.resb_release_reset());
        let transfer = !inner.rom_ctrl.block_start_status() && new_rom_ctrl.block_start_status();

        mask &= 0xDF7F7FFF;
        inner.rom_ctrl = ((u32::from(inner.rom_ctrl) & !mask) | (value & mask)).into();

        if !transfer {
            return;
        }

        let data_block_size = u8::from(inner.rom_ctrl.data_block_size());
        inner.block_size = match data_block_size {
            0 => 0,
            7 => 4,
            _ => 0x100 << data_block_size,
        };

        let cmd = u64::from_be(inner.bus_cmd_out);
        if inner.encrypted {
            todo!()
        }

        self.cmd_mode = CmdMode::None;
        if cmd == 0 {
            self.cmd_mode = CmdMode::Header;
            inner.read_addr = 0;
            self.set_read_buf_addr(0);
        } else if cmd == 0x9000000000000000 || (cmd >> 60) == 0x1 || cmd == 0xB800000000000000 {
            self.cmd_mode = CmdMode::Chip;
        } else if (cmd >> 56) == 0x3C {
            inner.encrypted = true;
        } else if (cmd >> 60) == 0x2 {
            self.cmd_mode = CmdMode::Secure;
            todo!()
        } else if (cmd >> 60) == 0xA {
            inner.encrypted = false;
        } else if (cmd >> 56) == 0xB7 {
            self.cmd_mode = CmdMode::Data;
            let mut read_addr = (((cmd >> 24) & 0xFFFFFFFF) as u32) % self.io.file_size;
            if read_addr < 0x8000 {
                read_addr = 0x8000 + (read_addr & 0x1FF);
            }
            inner.read_addr = read_addr;
            self.set_read_buf_addr(read_addr);
        } else if cmd != 0x9F00000000000000 {
            debug_println!("Unknown rom transfer command {:x}", cmd);
        }

        let inner = &mut self.inner[cpu];
        if inner.block_size == 0 {
            inner.rom_ctrl.set_data_word_status(false);
            inner.rom_ctrl.set_block_start_status(false);
            if inner.aux_spi_cnt.transfer_ready_irq() {
                get_cpu_regs_mut!(emu, cpu).send_interrupt(InterruptFlag::NdsSlotTransferCompletion, emu);
            }
        } else {
            inner.read_count = 0;
            get_cm_mut!(emu).schedule_imm(
                match cpu {
                    ARM9 => EventType::CartridgeWordReadArm9,
                    ARM7 => EventType::CartridgeWordReadArm7,
                },
                0,
            );
            get_mem_mut!(emu).breakout_imm = true;
        }
    }

    pub fn on_word_read_event<const CPU: CpuType>(_: &mut CycleManager, emu: &mut Emu, _: u16) {
        get_common_mut!(emu).cartridge.inner[CPU].rom_ctrl.set_data_word_status(true);
        io_dma!(emu, CPU).trigger_imm(DmaTransferMode::DsCartSlot, 0xF, emu);
    }
}

use crate::core::cpu_regs::InterruptFlag;
use crate::core::cycle_manager::ImmEventType;
use crate::core::emu::Emu;
use crate::core::memory::dma::DmaTransferMode;
use crate::core::CpuType;
use crate::logging::debug_println;
use crate::utils;
use crate::utils::HeapArrayU8;
use crate::{cartridge_io::CartridgeIo, utils::OptionWrapper};
use bilge::prelude::*;
use std::ops::Deref;

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
    pub io: OptionWrapper<CartridgeIo>,
    cmd_mode: CmdMode,
    inner: [CartridgeInner; 2],
    read_buf: HeapArrayU8<{ 16 * 1024 }>,
}

impl Cartridge {
    pub fn new() -> Self {
        Cartridge {
            io: OptionWrapper::none(),
            inner: [CartridgeInner::default(), CartridgeInner::default()],
            cmd_mode: CmdMode::None,
            read_buf: HeapArrayU8::default(),
        }
    }

    pub fn set_cartridge_io(&mut self, io: CartridgeIo) {
        self.io = OptionWrapper::new(io);
    }
}

impl Emu {
    pub fn cartridge_get_aux_spi_cnt(&self, cpu: CpuType) -> u16 {
        u16::from(self.cartridge.inner[cpu].aux_spi_cnt)
    }

    pub fn cartridge_get_aux_spi_data(&self, cpu: CpuType) -> u8 {
        self.cartridge.inner[cpu].aux_spi_data
    }

    pub fn cartridge_get_rom_ctrl(&mut self, cpu: CpuType) -> u32 {
        let ret = self.cartridge.inner[cpu].rom_ctrl.into();
        if !self.cartridge.inner[cpu].rom_ctrl.data_word_status() && self.cartridge.inner[cpu].rom_ctrl.block_start_status() {
            // Some games break when data word status is always on
            // Emulate cartridge read delay by toggling the status bit
            self.cartridge.inner[cpu].rom_ctrl.set_data_word_status(true);
        }
        ret
    }

    pub fn cartridge_get_rom_data_in(&mut self, cpu: CpuType) -> u32 {
        let inner = &mut self.cartridge.inner[cpu];
        if !inner.rom_ctrl.data_word_status() || !inner.rom_ctrl.block_start_status() {
            if inner.rom_ctrl.block_start_status() {
                // Some games break when data word status is always on
                // Emulate cartridge read delay by toggling the status bit
                inner.rom_ctrl.set_data_word_status(true);
            }
            return 0;
        }

        inner.rom_ctrl.set_data_word_status(false);
        inner.read_count += 4;
        if inner.read_count == inner.block_size {
            inner.rom_ctrl.set_block_start_status(false);
            if inner.aux_spi_cnt.transfer_ready_irq() {
                self.cpu_send_interrupt(cpu, InterruptFlag::NdsSlotTransferCompletion);
            }
        } else {
            self.cm.schedule_imm(ImmEventType::cartridge_word_read(cpu));
        }

        let inner = &mut self.cartridge.inner[cpu];
        match self.cartridge.cmd_mode {
            CmdMode::Header => {
                let offset = (inner.read_count as u32 - 4) & 0xFFF;
                utils::read_from_mem(self.cartridge.read_buf.deref(), offset)
            }
            CmdMode::Chip => 0x00001FC2,
            CmdMode::Secure => {
                todo!()
            }
            CmdMode::Data => {
                let offset = inner.read_count as u32 - 4;
                if offset + 3 < inner.block_size as u32 {
                    utils::read_from_mem(self.cartridge.read_buf.deref(), offset)
                } else {
                    0xFFFFFFFF
                }
            }
            CmdMode::None => 0xFFFFFFFF,
        }
    }

    pub fn cartridge_set_aux_spi_cnt(&mut self, cpu: CpuType, mut mask: u16, value: u16) {
        mask &= 0xE043;
        self.cartridge.inner[cpu].aux_spi_cnt = ((u16::from(self.cartridge.inner[cpu].aux_spi_cnt) & !mask) | (value & mask)).into();
    }

    pub fn cartridge_set_aux_spi_data(&mut self, cpu: CpuType, value: u8) {
        let inner = &mut self.cartridge.inner[cpu];

        if inner.aux_write_count == 0 {
            if value == 0 {
                return;
            }
            inner.aux_command = value;
            inner.aux_address = 0;
            inner.aux_spi_data = 0;
        } else {
            if self.cartridge.io.save_file_size == 0 {
                match inner.aux_command {
                    0x0B => {
                        self.cartridge.io.resize_save_file(0x200);
                        debug_println!("Detected EEPROM 0.5KB save type");
                    }
                    0x02 => {
                        self.cartridge.io.resize_save_file(0x10000);
                        debug_println!("Detected EEPROM 64KB save type");
                    }
                    0x0A => {
                        self.cartridge.io.resize_save_file(0x80000);
                        debug_println!("Detected FLASH 512KB save type");
                    }
                    _ => {}
                }
            }

            let save_size = self.cartridge.io.save_file_size;
            match save_size {
                0x200 => match inner.aux_command {
                    0x03 | 0x0B => {
                        if inner.aux_write_count < 2 {
                            inner.aux_address = value as u32;
                            inner.aux_spi_data = 0;
                        } else {
                            let addr_offset = if inner.aux_command == 0x0B { 0x100 } else { 0 };
                            inner.aux_spi_data = self.cartridge.io.read_save_buf((inner.aux_address + addr_offset) & (save_size - 1));
                            inner.aux_address += 1;
                        }
                    }
                    0x02 | 0x0A => {
                        if inner.aux_write_count < 2 {
                            inner.aux_address = value as u32;
                            inner.aux_spi_data = 0;
                        } else {
                            let addr_offset = if inner.aux_command == 0x0A { 0x100 } else { 0 };
                            self.cartridge.io.write_save_buf((inner.aux_address + addr_offset) & (save_size - 1), value);
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
                            inner.aux_spi_data = if inner.aux_address < save_size { self.cartridge.io.read_save_buf(inner.aux_address) } else { 0 };
                            inner.aux_address += 1;
                        }
                    }
                    0x02 => {
                        if inner.aux_write_count < if save_size == 0x20000 { 4 } else { 3 } {
                            inner.aux_address |= (value as u32) << ((if save_size == 0x20000 { 3 } else { 2 } - inner.aux_write_count) * 8);
                            inner.aux_spi_data = 0;
                        } else {
                            if inner.aux_address < save_size {
                                self.cartridge.io.write_save_buf(inner.aux_address, value);
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
                            inner.aux_spi_data = if inner.aux_address < save_size { self.cartridge.io.read_save_buf(inner.aux_address) } else { 0 };
                            inner.aux_address += 1;
                        }
                    }
                    0x0A => {
                        if inner.aux_write_count < 4 {
                            inner.aux_address |= (value as u32) << ((3 - inner.aux_write_count) * 8);
                            inner.aux_spi_data = 0;
                        } else {
                            if inner.aux_address < save_size {
                                self.cartridge.io.write_save_buf(inner.aux_address, value);
                            }
                            inner.aux_address += 1;
                            inner.aux_spi_data = 0;
                        }
                    }
                    0x08 => inner.aux_spi_data = if self.cartridge.io.header.game_code[0] == b'I' { 0xAA } else { 0 },
                    _ => {
                        debug_println!("Unknown FLASH command {:x}", inner.aux_command);
                        inner.aux_spi_data = 0;
                    }
                },
                _ => {
                    debug_println!("Unknown api size {:x}", self.cartridge.io.save_file_size)
                }
            }
        }

        if inner.aux_spi_cnt.hold_chipselect() {
            inner.aux_write_count += 1;
        } else {
            inner.aux_write_count = 0;
        }
    }

    pub fn cartridge_set_bus_cmd_out_l(&mut self, cpu: CpuType, mask: u32, value: u32) {
        self.cartridge.inner[cpu].bus_cmd_out = (self.cartridge.inner[cpu].bus_cmd_out & !(mask as u64)) | (value & mask) as u64;
    }

    pub fn cartridge_set_bus_cmd_out_h(&mut self, cpu: CpuType, mask: u32, value: u32) {
        self.cartridge.inner[cpu].bus_cmd_out = (self.cartridge.inner[cpu].bus_cmd_out & !((mask as u64) << 32)) | ((value & mask) as u64) << 32;
    }

    pub fn cartridge_set_rom_ctrl(&mut self, cpu: CpuType, mut mask: u32, value: u32) {
        let new_rom_ctrl = RomCtrl::from(value);
        let inner = &mut self.cartridge.inner[cpu];

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

        self.cartridge.cmd_mode = CmdMode::None;
        if cmd == 0 {
            self.cartridge.cmd_mode = CmdMode::Header;
            self.cartridge.io.read_slice(0, &mut self.cartridge.read_buf[..inner.block_size as usize]).unwrap();
        } else if cmd == 0x9000000000000000 || (cmd >> 60) == 0x1 || cmd == 0xB800000000000000 {
            self.cartridge.cmd_mode = CmdMode::Chip;
        } else if (cmd >> 56) == 0x3C {
            inner.encrypted = true;
        } else if (cmd >> 60) == 0x2 {
            self.cartridge.cmd_mode = CmdMode::Secure;
            todo!()
        } else if (cmd >> 60) == 0xA {
            inner.encrypted = false;
        } else if (cmd >> 56) == 0xB7 {
            self.cartridge.cmd_mode = CmdMode::Data;
            let mut read_addr = (((cmd >> 24) & 0xFFFFFFFF) as u32) % self.cartridge.io.file_size;
            if read_addr < 0x8000 {
                read_addr = 0x8000 + (read_addr & 0x1FF);
            }
            self.cartridge.io.read_slice(read_addr, &mut self.cartridge.read_buf[..inner.block_size as usize]).unwrap();
        } else if cmd != 0x9F00000000000000 {
            debug_println!("Unknown rom transfer command {:x}", cmd);
        }

        if inner.block_size == 0 {
            inner.rom_ctrl.set_data_word_status(false);
            inner.rom_ctrl.set_block_start_status(false);
            if inner.aux_spi_cnt.transfer_ready_irq() {
                self.cpu_send_interrupt(cpu, InterruptFlag::NdsSlotTransferCompletion);
            }
        } else {
            inner.read_count = 0;
            self.cm.schedule_imm(ImmEventType::cartridge_word_read(cpu));
        }
    }

    pub fn cartridge_on_word_read_event<const CPU: CpuType>(&mut self) {
        self.cartridge.inner[CPU].rom_ctrl.set_data_word_status(true);
        self.dma_trigger_imm(CPU, DmaTransferMode::DsCartSlot, 0xF);
    }
}

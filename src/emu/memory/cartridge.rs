use crate::cartridge_reader::CartridgeReader;
use crate::emu::cpu_regs::InterruptFlag;
use crate::emu::cycle_manager::EventType;
use crate::emu::emu::{get_cm_mut, get_common_mut, get_cpu_regs_mut, io_dma, Emu};
use crate::emu::memory::dma::DmaTransferMode;
use crate::emu::CpuType;
use crate::logging::debug_println;
use bilge::prelude::*;

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
struct AuxSpiCnt {
    baudrate: u2,
    not_used: u4,
    hold_chipselect: u1,
    busy: u1,
    not_used2: u5,
    nds_slot_mode: u1,
    transfer_ready_irq: u1,
    nds_slot_enable: u1,
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
    data_word_status: u1,
    data_block_size: u3,
    transfer_clk_rate: u1,
    key1_gap_clks: u1,
    resb_release_reset: u1,
    wr: u1,
    block_start_status: u1,
}

#[derive(Debug)]
enum CmdMode {
    Header,
    Chip,
    Secure,
    Data,
    None,
}

struct CartridgeInner {
    aux_spi_cnt: AuxSpiCnt,
    aux_spi_data: u8,
    bus_cmd_out: u64,
    rom_ctrl: RomCtrl,
    word_cycles: u8,
    block_size: u16,
    encrypted: bool,
    read_count: u16,
}

impl CartridgeInner {
    fn new() -> Self {
        CartridgeInner {
            aux_spi_cnt: AuxSpiCnt::from(0),
            aux_spi_data: 0,
            bus_cmd_out: 0,
            rom_ctrl: RomCtrl::from(0),
            word_cycles: 0,
            block_size: 0,
            encrypted: false,
            read_count: 0,
        }
    }
}

pub struct Cartridge {
    pub reader: CartridgeReader,
    cmd_mode: CmdMode,
    inner: [CartridgeInner; 2],
    read_buf: Vec<u8>,
}

impl Cartridge {
    pub fn new(cartridge: CartridgeReader) -> Self {
        Cartridge {
            reader: cartridge,
            inner: [CartridgeInner::new(), CartridgeInner::new()],
            cmd_mode: CmdMode::None,
            read_buf: Vec::new(),
        }
    }

    pub fn get_aux_spi_cnt<const CPU: CpuType>(&self) -> u16 {
        u16::from(self.inner[CPU].aux_spi_cnt)
    }

    pub fn get_aux_spi_data<const CPU: CpuType>(&self) -> u8 {
        self.inner[CPU].aux_spi_data
    }

    pub fn get_rom_ctrl<const CPU: CpuType>(&self) -> u32 {
        self.inner[CPU].rom_ctrl.into()
    }

    pub fn get_rom_data_in<const CPU: CpuType>(&mut self, emu: &mut Emu) -> u32 {
        let inner = &mut self.inner[CPU];
        if !bool::from(inner.rom_ctrl.data_word_status()) {
            return 0;
        }

        inner.rom_ctrl.set_data_word_status(u1::new(0));
        inner.read_count += 4;
        if inner.read_count == inner.block_size {
            inner.rom_ctrl.set_block_start_status(u1::new(0));
            if bool::from(inner.aux_spi_cnt.transfer_ready_irq()) {
                get_cpu_regs_mut!(emu, CPU)
                    .send_interrupt(InterruptFlag::NdsSlotTransferCompletion, get_cm_mut!(emu));
            }
        } else {
            get_cm_mut!(emu).schedule(
                inner.word_cycles as u32,
                match CPU {
                    CpuType::ARM9 => EventType::CartridgeWordReadArm9,
                    CpuType::ARM7 => EventType::CartridgeWordReadArm7,
                },
            );
        }

        match self.cmd_mode {
            CmdMode::Header => {
                let offset = ((inner.read_count as u32 - 4) & 0xFFF) as usize;
                let mut buf = [0u8; 4];
                buf.copy_from_slice(&self.read_buf[offset..offset + 4]);
                u32::from_le_bytes(buf)
            }
            CmdMode::Chip => 0x00001FC2,
            CmdMode::Secure => {
                todo!()
            }
            CmdMode::Data => {
                let offset = (inner.read_count as u32 - 4) as usize;
                if offset + 3 < self.read_buf.len() {
                    let mut buf = [0u8; 4];
                    buf.copy_from_slice(&self.read_buf[offset..offset + 4]);
                    u32::from_le_bytes(buf)
                } else {
                    0xFFFFFFFF
                }
            }
            CmdMode::None => 0xFFFFFFFF,
        }
    }

    pub fn set_aux_spi_cnt<const CPU: CpuType>(&mut self, mut mask: u16, value: u16) {
        mask &= 0xE043;
        self.inner[CPU].aux_spi_cnt =
            ((u16::from(self.inner[CPU].aux_spi_cnt) & !mask) | (value & mask)).into();
    }

    pub fn set_aux_spi_data<const CPU: CpuType>(&self, value: u8) {}

    pub fn set_bus_cmd_out_l<const CPU: CpuType>(&mut self, mask: u32, value: u32) {
        self.inner[CPU].bus_cmd_out =
            (self.inner[CPU].bus_cmd_out & !(mask as u64)) | (value & mask) as u64;
    }

    pub fn set_bus_cmd_out_h<const CPU: CpuType>(&mut self, mask: u32, value: u32) {
        self.inner[CPU].bus_cmd_out =
            (self.inner[CPU].bus_cmd_out & !((mask as u64) << 32)) | ((value & mask) as u64) << 32;
    }

    pub fn set_rom_ctrl<const CPU: CpuType>(&mut self, mut mask: u32, value: u32, emu: &mut Emu) {
        let new_rom_ctrl = RomCtrl::from(value);
        let inner = &mut self.inner[CPU];

        inner
            .rom_ctrl
            .set_resb_release_reset(new_rom_ctrl.resb_release_reset());
        let transfer = !bool::from(inner.rom_ctrl.block_start_status())
            && bool::from(new_rom_ctrl.block_start_status());

        mask &= 0xDF7F7FFF;
        inner.rom_ctrl = ((u32::from(inner.rom_ctrl) & !mask) | (value & mask)).into();

        inner.word_cycles = if bool::from(inner.rom_ctrl.transfer_clk_rate()) {
            4 * 8
        } else {
            4 * 5
        };

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
            self.read_buf.resize(inner.block_size as usize, 0);
            self.reader.read_slice(0, &mut self.read_buf);
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
            let mut read_addr = (((cmd >> 24) & 0xFFFFFFFF) as u32) % self.reader.file_size;
            if read_addr < 0x8000 {
                read_addr = 0x8000 + (read_addr & 0x1FF);
            }
            self.read_buf.resize(inner.block_size as usize, 0);
            self.reader.read_slice(read_addr, &mut self.read_buf);
        } else if cmd != 0x9F00000000000000 {
            debug_println!("Unknown rom transfer command {:x}", cmd);
        }

        if inner.block_size == 0 {
            inner.rom_ctrl.set_data_word_status(u1::new(0));
            inner.rom_ctrl.set_block_start_status(u1::new(0));
            if bool::from(inner.aux_spi_cnt.transfer_ready_irq()) {
                get_cpu_regs_mut!(emu, CPU)
                    .send_interrupt(InterruptFlag::NdsSlotTransferCompletion, get_cm_mut!(emu));
            }
        } else {
            get_cm_mut!(emu).schedule(
                inner.word_cycles as u32,
                match CPU {
                    CpuType::ARM9 => EventType::CartridgeWordReadArm9,
                    CpuType::ARM7 => EventType::CartridgeWordReadArm7,
                },
            );
            inner.read_count = 0;
        }
    }

    pub fn on_word_read_event<const CPU: CpuType>(emu: &mut Emu) {
        get_common_mut!(emu).cartridge.inner[CPU]
            .rom_ctrl
            .set_data_word_status(u1::new(1));
        io_dma!(emu, CPU).trigger_all(DmaTransferMode::DsCartSlot, get_cm_mut!(emu));
    }
}

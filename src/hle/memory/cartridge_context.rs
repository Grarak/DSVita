use crate::cartridge::Cartridge;
use crate::hle::cpu_regs::{CpuRegsContainer, InterruptFlag};
use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::memory::dma::{Dma, DmaContainer, DmaTransferMode};
use crate::hle::CpuType;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

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

enum CmdMode {
    Header,
    Chip,
    Secure,
    Data,
    None,
}

struct CartridgeInner {
    aux_spi_cnt: AuxSpiCnt,
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
            bus_cmd_out: 0,
            rom_ctrl: RomCtrl::from(0),
            word_cycles: 0,
            block_size: 0,
            encrypted: false,
            read_count: 0,
        }
    }
}

pub struct CartridgeContext {
    cycle_manager: Rc<CycleManager>,
    cpu_regs: CpuRegsContainer,
    dma: DmaContainer,
    pub cartridge: Cartridge,
    cmd_mode: CmdMode,
    inner: [Rc<RefCell<CartridgeInner>>; 2],
    read_data: Vec<u8>,
}

impl CartridgeContext {
    pub fn new(
        cycle_manager: Rc<CycleManager>,
        cpu_regs: CpuRegsContainer,
        dma: DmaContainer,
        cartridge: Cartridge,
    ) -> Self {
        CartridgeContext {
            cycle_manager,
            cpu_regs,
            dma,
            cartridge,
            inner: [
                Rc::new(RefCell::new(CartridgeInner::new())),
                Rc::new(RefCell::new(CartridgeInner::new())),
            ],
            cmd_mode: CmdMode::None,
            read_data: Vec::new(),
        }
    }

    pub fn get_aux_spi_cnt<const CPU: CpuType>(&self) -> u16 {
        u16::from(self.inner[CPU].borrow().aux_spi_cnt)
    }

    pub fn get_rom_ctrl<const CPU: CpuType>(&self) -> u32 {
        self.inner[CPU].borrow().rom_ctrl.into()
    }

    pub fn get_rom_data_in<const CPU: CpuType>(&self) -> u32 {
        let mut inner = self.inner[CPU].borrow_mut();
        if !bool::from(inner.rom_ctrl.data_word_status()) {
            return 0;
        }

        inner.rom_ctrl.set_data_word_status(u1::new(0));
        inner.read_count += 4;
        if inner.read_count == inner.block_size {
            inner.rom_ctrl.set_block_start_status(u1::new(0));
            if bool::from(inner.aux_spi_cnt.transfer_ready_irq()) {
                self.cpu_regs
                    .send_interrupt::<CPU>(InterruptFlag::NdsSlotTransferCompletion);
            }
        } else {
            self.cycle_manager.schedule(
                inner.word_cycles as u32,
                Box::new(WordReadEvent::new(
                    self.dma.get::<CPU>(),
                    self.inner[CPU].clone(),
                )),
            );
        }

        match self.cmd_mode {
            CmdMode::Header => {
                let offset = (inner.read_count as usize - 4) & 0xFFF;
                u32::from_le_bytes(
                    self.read_data.as_slice()[offset..offset + 4]
                        .try_into()
                        .unwrap(),
                )
            }
            CmdMode::Chip => 0x00001FC2,
            CmdMode::Secure => {
                todo!()
            }
            CmdMode::Data => {
                let offset = inner.read_count as usize - 4;
                if offset + 4 < self.read_data.len() {
                    u32::from_le_bytes(
                        self.read_data.as_slice()[offset..offset + 4]
                            .try_into()
                            .unwrap(),
                    )
                } else {
                    0xFFFFFFFF
                }
            }
            CmdMode::None => 0xFFFFFFFF,
        }
    }

    pub fn set_aux_spi_cnt<const CPU: CpuType>(&self, mut mask: u16, value: u16) {
        mask &= 0xE043;
        let mut inner = self.inner[CPU].borrow_mut();
        inner.aux_spi_cnt = ((u16::from(inner.aux_spi_cnt) & !mask) | (value & mask)).into();
    }

    pub fn set_bus_cmd_out_l<const CPU: CpuType>(&self, mask: u32, value: u32) {
        let mut inner = self.inner[CPU].borrow_mut();
        inner.bus_cmd_out = (inner.bus_cmd_out & !(mask as u64)) | (value & mask) as u64;
    }

    pub fn set_bus_cmd_out_h<const CPU: CpuType>(&self, mask: u32, value: u32) {
        let mut inner = self.inner[CPU].borrow_mut();
        inner.bus_cmd_out =
            (inner.bus_cmd_out & !((mask as u64) << 32)) | ((value & mask) as u64) << 32;
    }

    pub fn set_rom_ctrl<const CPU: CpuType>(&mut self, mut mask: u32, value: u32) {
        let new_rom_ctrl = RomCtrl::from(value);
        let mut inner = self.inner[CPU].borrow_mut();

        inner
            .rom_ctrl
            .set_resb_release_reset(new_rom_ctrl.resb_release_reset());
        let transfer = !bool::from(inner.rom_ctrl.block_start_status())
            && bool::from(new_rom_ctrl.block_start_status());

        mask &= 0xDF7F7FFF;
        inner.rom_ctrl = ((u32::from(inner.rom_ctrl) | !mask) | (value & mask)).into();

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
            let mut addr = (((cmd >> 24) & 0xFFFFFFFF) as u32) % self.cartridge.file_size;
            if addr < 0x8000 {
                addr = 0x8000 + (addr & 0x1FF);
            }
            self.read_data.resize(inner.block_size as usize, 0);
            self.cartridge.read_slice(addr, &mut self.read_data);
        } else if cmd != 0x9F00000000000000 {
            debug_println!("Unknown rom transfer command {:x}", cmd);
        }

        if inner.block_size == 0 {
            inner.rom_ctrl.set_data_word_status(u1::new(0));
            inner.rom_ctrl.set_block_start_status(u1::new(0));
            if bool::from(inner.aux_spi_cnt.transfer_ready_irq()) {
                self.cpu_regs
                    .send_interrupt::<CPU>(InterruptFlag::NdsSlotTransferCompletion);
            }
        } else {
            self.cycle_manager.schedule(
                inner.word_cycles as u32,
                Box::new(WordReadEvent::new(
                    self.dma.get::<CPU>(),
                    self.inner[CPU].clone(),
                )),
            );
            inner.read_count = 0;
        }
    }
}

pub struct WordReadEvent<const CPU: CpuType> {
    dma: Rc<RefCell<Dma<CPU>>>,
    inner: Rc<RefCell<CartridgeInner>>,
}

impl<const CPU: CpuType> WordReadEvent<CPU> {
    fn new(dma: Rc<RefCell<Dma<CPU>>>, inner: Rc<RefCell<CartridgeInner>>) -> Self {
        WordReadEvent { dma, inner }
    }
}

impl<const CPU: CpuType> CycleEvent for WordReadEvent<CPU> {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, _: u16) {
        self.inner
            .borrow_mut()
            .rom_ctrl
            .set_data_word_status(u1::new(1));
        self.dma.borrow().trigger_all(DmaTransferMode::DsCartSlot);
    }
}

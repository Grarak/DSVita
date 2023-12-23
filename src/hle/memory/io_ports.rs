use crate::hle::cpu_regs::CpuRegs;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::dma::Dma;
use crate::hle::memory::memory::Memory;
use crate::hle::memory::Convert;
use crate::hle::spi_context::SpiContext;
use crate::hle::spu_context::SpuContext;
use crate::hle::timers_context::TimersContext;
use crate::hle::CpuType;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct IoPorts {
    cpu_type: CpuType,
    memory: Arc<RwLock<Memory>>,
    spi_context: Arc<RwLock<SpiContext>>,
    ipc_handler: Arc<RwLock<IpcHandler>>,
    cpu_regs: Rc<RefCell<CpuRegs>>,
    gpu_context: Rc<RefCell<GpuContext>>,
    spu_context: Rc<RefCell<SpuContext>>,
    pub dma: Arc<RefCell<Dma>>,
    timers_context: Rc<RefCell<TimersContext>>,
}

impl IoPorts {
    pub fn new(
        cpu_type: CpuType,
        memory: Arc<RwLock<Memory>>,
        spi_context: Arc<RwLock<SpiContext>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
        cpu_regs: Rc<RefCell<CpuRegs>>,
        gpu_context: Rc<RefCell<GpuContext>>,
        spu_context: Rc<RefCell<SpuContext>>,
        dma: Arc<RefCell<Dma>>,
        timers_context: Rc<RefCell<TimersContext>>,
    ) -> Self {
        IoPorts {
            cpu_type,
            memory,
            ipc_handler,
            spi_context,
            cpu_regs,
            gpu_context,
            spu_context,
            dma,
            timers_context,
        }
    }

    pub fn write<T: Convert>(&self, addr_offset: u32, value: T) {
        match self.cpu_type {
            CpuType::ARM9 => self.write_arm9(addr_offset, value),
            CpuType::ARM7 => self.write_arm7(addr_offset, value),
        }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        match self.cpu_type {
            CpuType::ARM9 => self.read_arm9(addr_offset),
            CpuType::ARM7 => self.read_arm7(addr_offset),
        }
    }

    fn read_common<T: Convert>(&self, addr_offset: u32) -> T {
        T::from(match addr_offset {
            0xB8 => self.dma.borrow().get_cnt(0),

            0xDC => self.dma.borrow().get_cnt(3),

            0x180 => self.ipc_handler.read().unwrap().get_sync_reg(self.cpu_type) as u32,
            0x208 => self.cpu_regs.borrow().ime as u32,
            _ => todo!("io port read {:x}", addr_offset),
        })
    }

    fn read_arm7<T: Convert>(&self, addr_offset: u32) -> T {
        T::from(match addr_offset {
            0x180 => self.ipc_handler.read().unwrap().get_sync_reg(CpuType::ARM7) as u32,
            0x1C0 => self.spi_context.read().unwrap().cnt as u32,
            0x1C2 => self.spi_context.read().unwrap().data as u32,
            0x304 => 0,
            0x500 => self.spu_context.borrow().main_sound_cnt as u32,
            _ => self.read_common(addr_offset),
        })
    }

    fn read_arm9<T: Convert>(&self, addr_offset: u32) -> T {
        T::from(match addr_offset {
            0x4000 => self.spu_context.borrow().get_cnt(0),
            0x4008 => 0,
            _ => self.read_common(addr_offset),
        })
    }

    fn write_common<T: Convert>(&self, addr_offset: u32, value: T) {
        let value = value.into();
        match addr_offset {
            0xB0 => self.dma.borrow_mut().set_sad(0, value.into()),
            0xB4 => self.dma.borrow_mut().set_dad(0, value.into()),
            0xB8 => self.dma.borrow_mut().set_cnt(0, value.into()),

            0xD4 => self.dma.borrow_mut().set_sad(3, value.into()),
            0xD8 => self.dma.borrow_mut().set_dad(3, value.into()),
            0xDC => self.dma.borrow_mut().set_cnt(3, value.into()),

            0x208 => self.cpu_regs.borrow_mut().set_ime(value as u8),
            0x300 => self.cpu_regs.borrow_mut().set_post_flg(value as u8),
            _ => todo!("io port write {:x}", addr_offset),
        };
    }

    #[rustfmt::skip]
    fn write_arm7<T: Convert>(&self, addr_offset: u32, value: T) {
        match addr_offset {
            0x180 => self.ipc_handler.write().unwrap().set_sync_reg(CpuType::ARM7, value.into() as u16),
            0x1C0 => self.spi_context.write().unwrap().set_cnt(value.into() as u16),
            0x1C2 => self.spi_context.write().unwrap().set_data(value.into() as u8),
            0x304 => {},

            0x400 => self.spu_context.borrow_mut().set_cnt(0, value.into()),
            0x404 => self.spu_context.borrow_mut().set_sad(0, value.into()),
            0x408 => self.spu_context.borrow_mut().set_tmr(0, value.into() as u16),
            0x40A => self.spu_context.borrow_mut().set_pnt(0, value.into() as u16),
            0x40C => self.spu_context.borrow_mut().set_len(0, value.into()),

            0x410 => self.spu_context.borrow_mut().set_cnt(1, value.into()),
            0x414 => self.spu_context.borrow_mut().set_sad(1, value.into()),
            0x418 => self.spu_context.borrow_mut().set_tmr(1, value.into() as u16),
            0x41A => self.spu_context.borrow_mut().set_pnt(1, value.into() as u16),
            0x41C => self.spu_context.borrow_mut().set_len(1, value.into()),

            0x420 => self.spu_context.borrow_mut().set_cnt(2, value.into()),
            0x424 => self.spu_context.borrow_mut().set_sad(2, value.into()),
            0x428 => self.spu_context.borrow_mut().set_tmr(2, value.into() as u16),
            0x42A => self.spu_context.borrow_mut().set_pnt(2, value.into() as u16),
            0x42C => self.spu_context.borrow_mut().set_len(2, value.into()),

            0x430 => self.spu_context.borrow_mut().set_cnt(3, value.into()),
            0x434 => self.spu_context.borrow_mut().set_sad(3, value.into()),
            0x438 => self.spu_context.borrow_mut().set_tmr(3, value.into() as u16),
            0x43A => self.spu_context.borrow_mut().set_pnt(3, value.into() as u16),
            0x43C => self.spu_context.borrow_mut().set_len(3, value.into()),

            0x440 => self.spu_context.borrow_mut().set_cnt(4, value.into()),
            0x444 => self.spu_context.borrow_mut().set_sad(4, value.into()),
            0x448 => self.spu_context.borrow_mut().set_tmr(4, value.into() as u16),
            0x44A => self.spu_context.borrow_mut().set_pnt(4, value.into() as u16),
            0x44C => self.spu_context.borrow_mut().set_len(4, value.into()),

            0x450 => self.spu_context.borrow_mut().set_cnt(5, value.into()),
            0x454 => self.spu_context.borrow_mut().set_sad(5, value.into()),
            0x458 => self.spu_context.borrow_mut().set_tmr(5, value.into() as u16),
            0x45A => self.spu_context.borrow_mut().set_pnt(5, value.into() as u16),
            0x45C => self.spu_context.borrow_mut().set_len(5, value.into()),

            0x460 => self.spu_context.borrow_mut().set_cnt(6, value.into()),
            0x464 => self.spu_context.borrow_mut().set_sad(6, value.into()),
            0x468 => self.spu_context.borrow_mut().set_tmr(6, value.into() as u16),
            0x46A => self.spu_context.borrow_mut().set_pnt(6, value.into() as u16),
            0x46C => self.spu_context.borrow_mut().set_len(6, value.into()),

            0x470 => self.spu_context.borrow_mut().set_cnt(7, value.into()),
            0x474 => self.spu_context.borrow_mut().set_sad(7, value.into()),
            0x478 => self.spu_context.borrow_mut().set_tmr(7, value.into() as u16),
            0x47A => self.spu_context.borrow_mut().set_pnt(7, value.into() as u16),
            0x47C => self.spu_context.borrow_mut().set_len(7, value.into()),

            0x480 => self.spu_context.borrow_mut().set_cnt(8, value.into()),
            0x484 => self.spu_context.borrow_mut().set_sad(8, value.into()),
            0x488 => self.spu_context.borrow_mut().set_tmr(8, value.into() as u16),
            0x48A => self.spu_context.borrow_mut().set_pnt(8, value.into() as u16),
            0x48C => self.spu_context.borrow_mut().set_len(8, value.into()),

            0x490 => self.spu_context.borrow_mut().set_cnt(9, value.into()),
            0x494 => self.spu_context.borrow_mut().set_sad(9, value.into()),
            0x498 => self.spu_context.borrow_mut().set_tmr(9, value.into() as u16),
            0x49A => self.spu_context.borrow_mut().set_pnt(9, value.into() as u16),
            0x49C => self.spu_context.borrow_mut().set_len(9, value.into()),

            0x4A0 => self.spu_context.borrow_mut().set_cnt(10, value.into()),
            0x4A4 => self.spu_context.borrow_mut().set_sad(10, value.into()),
            0x4A8 => self.spu_context.borrow_mut().set_tmr(10, value.into() as u16),
            0x4AA => self.spu_context.borrow_mut().set_pnt(10, value.into() as u16),
            0x4AC => self.spu_context.borrow_mut().set_len(10, value.into()),

            0x4B0 => self.spu_context.borrow_mut().set_cnt(11, value.into()),
            0x4B4 => self.spu_context.borrow_mut().set_sad(11, value.into()),
            0x4B8 => self.spu_context.borrow_mut().set_tmr(11, value.into() as u16),
            0x4BA => self.spu_context.borrow_mut().set_pnt(11, value.into() as u16),
            0x4BC => self.spu_context.borrow_mut().set_len(11, value.into()),

            0x4C0 => self.spu_context.borrow_mut().set_cnt(12, value.into()),
            0x4C4 => self.spu_context.borrow_mut().set_sad(12, value.into()),
            0x4C8 => self.spu_context.borrow_mut().set_tmr(12, value.into() as u16),
            0x4CA => self.spu_context.borrow_mut().set_pnt(12, value.into() as u16),
            0x4CC => self.spu_context.borrow_mut().set_len(12, value.into()),

            0x4D0 => self.spu_context.borrow_mut().set_cnt(13, value.into()),
            0x4D4 => self.spu_context.borrow_mut().set_sad(13, value.into()),
            0x4D8 => self.spu_context.borrow_mut().set_tmr(13, value.into() as u16),
            0x4DA => self.spu_context.borrow_mut().set_pnt(13, value.into() as u16),
            0x4DC => self.spu_context.borrow_mut().set_len(13, value.into()),

            0x4E0 => self.spu_context.borrow_mut().set_cnt(14, value.into()),
            0x4E4 => self.spu_context.borrow_mut().set_sad(14, value.into()),
            0x4E8 => self.spu_context.borrow_mut().set_tmr(14, value.into() as u16),
            0x4EA => self.spu_context.borrow_mut().set_pnt(14, value.into() as u16),
            0x4EC => self.spu_context.borrow_mut().set_len(14, value.into()),

            0x4F0 => self.spu_context.borrow_mut().set_cnt(15, value.into()),
            0x4F4 => self.spu_context.borrow_mut().set_sad(15, value.into()),
            0x4F8 => self.spu_context.borrow_mut().set_tmr(15, value.into() as u16),
            0x4FA => self.spu_context.borrow_mut().set_pnt(15, value.into() as u16),
            0x4FC => self.spu_context.borrow_mut().set_len(15, value.into()),

            0x500 => self.spu_context.borrow_mut().set_main_sound_cnt(value.into() as u16),
            0x504 => self.spu_context.borrow_mut().set_sound_bias(value.into() as u16),
            _ => self.write_common(addr_offset, value),
        }
    }

    #[rustfmt::skip]
    fn write_arm9<T: Convert>(&self, addr_offset: u32, value: T) {
        match addr_offset {
            0x180 => self.ipc_handler.write().unwrap().set_sync_reg(CpuType::ARM9, value.into() as u16),
            0x247 => self.memory.write().unwrap().set_wram_cnt(value.into() as u8),
            0x304 => self.gpu_context.borrow_mut().set_pow_cnt1(value.into() as u16),
            _ => self.write_common(addr_offset, value),
        }
    }
}

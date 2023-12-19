use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::dma::Dma;
use crate::hle::memory::handler::Convert;
use crate::hle::memory::memory::Memory;
use crate::hle::registers::ThreadRegs;
use crate::hle::spu_context::SpuContext;
use crate::hle::CpuType;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct IoPorts {
    cpu_type: CpuType,
    memory: Arc<RwLock<Memory>>,
    ipc_handler: Arc<RwLock<IpcHandler>>,
    thread_regs: Rc<RefCell<ThreadRegs>>,
    gpu_context: Rc<RefCell<GpuContext>>,
    spu_context: Rc<RefCell<SpuContext>>,
    dma: Dma,
}

impl IoPorts {
    pub fn new(
        cpu_type: CpuType,
        memory: Arc<RwLock<Memory>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        gpu_context: Rc<RefCell<GpuContext>>,
        spu_context: Rc<RefCell<SpuContext>>,
    ) -> Self {
        IoPorts {
            cpu_type,
            memory,
            ipc_handler,
            thread_regs,
            gpu_context,
            spu_context,
            dma: Dma::new(cpu_type),
        }
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
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
            0x180 => self.ipc_handler.read().unwrap().get_sync_reg(self.cpu_type) as u32,
            _ => todo!("io port read {:x}", addr_offset),
        })
    }

    fn read_arm7<T: Convert>(&self, addr_offset: u32) -> T {
        T::from(match addr_offset {
            0x180 => self.ipc_handler.read().unwrap().get_sync_reg(CpuType::ARM7) as u32,
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

    fn write_common<T: Convert>(&mut self, cpu_type: CpuType, addr_offset: u32, value: T) {
        let value = value.into();
        match addr_offset {
            0xd4 => self.dma.set_sad(3, value.into()),
            0xd8 => self.dma.set_dad(3, value.into()),
            0xdc => self.dma.set_cnt(3, value.into()),
            0x208 => self.thread_regs.borrow_mut().set_ime(value as u8),
            0x300 => self.thread_regs.borrow_mut().set_post_flg(value as u8),
            _ => todo!("{:?} unimplemented io port {:x}", cpu_type, addr_offset),
        };
    }

    fn write_arm7<T: Convert>(&mut self, addr_offset: u32, value: T) {
        match addr_offset {
            0x180 => self
                .ipc_handler
                .write()
                .unwrap()
                .set_sync_reg(CpuType::ARM7, value.into() as u16),
            0x504 => self
                .spu_context
                .borrow_mut()
                .set_sound_bias(value.into() as u16),
            _ => self.write_common(CpuType::ARM7, addr_offset, value),
        }
    }

    fn write_arm9<T: Convert>(&mut self, addr_offset: u32, value: T) {
        match addr_offset {
            0x180 => self
                .ipc_handler
                .write()
                .unwrap()
                .set_sync_reg(CpuType::ARM9, value.into() as u16),
            0x247 => self
                .memory
                .write()
                .unwrap()
                .set_wram_cnt(value.into() as u8),
            0x304 => self
                .gpu_context
                .borrow_mut()
                .set_pow_cnt1(value.into() as u16),
            _ => self.write_common(CpuType::ARM9, addr_offset, value),
        }
    }
}

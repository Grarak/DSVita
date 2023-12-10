use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::memory::indirect_memory::indirect_mem_handler::WriteBack;
use crate::hle::memory::memory::Memory;
use crate::hle::registers::ThreadRegs;
use crate::hle::spu_context::SpuContext;
use crate::hle::CpuType;
use std::cell::RefCell;
use std::rc::Rc;

pub struct IoPorts {
    memory: Rc<RefCell<Memory>>,
    thread_regs: Rc<RefCell<ThreadRegs>>,
    gpu_context: Rc<RefCell<GpuContext>>,
    spu_context: Rc<RefCell<SpuContext>>,
}

impl IoPorts {
    pub fn new(
        memory: Rc<RefCell<Memory>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        gpu_context: Rc<RefCell<GpuContext>>,
        spu_context: Rc<RefCell<SpuContext>>,
    ) -> Self {
        IoPorts {
            memory,
            thread_regs,
            gpu_context,
            spu_context,
        }
    }

    pub fn read_arm7<T: Into<u32>>(&self, addr_offset: u32) -> T {
        match addr_offset {
            _ => todo!("unimplemented io port read {:x}", addr_offset),
        }
    }

    fn write_common<T: Into<u32>>(
        &self,
        cpu_type: CpuType,
        addr_offset: u32,
        value: T,
    ) -> WriteBack {
        let value = value.into();
        match addr_offset {
            0x180 => todo!(),
            0x208 => WriteBack::Byte(self.thread_regs.borrow_mut().set_ime(value as u8)),
            0x300 => WriteBack::Byte(self.thread_regs.borrow_mut().set_post_flg(value as u8)),
            _ => todo!("{:?} unimplemented io port {:x}", cpu_type, addr_offset),
        }
    }

    pub fn write_arm7<T: Into<u32>>(&self, addr_offset: u32, value: T) -> WriteBack {
        match addr_offset {
            0x504 => WriteBack::Half(
                self.spu_context
                    .borrow_mut()
                    .set_sound_bias(value.into() as u16),
            ),
            _ => self.write_common(CpuType::ARM7, addr_offset, value),
        }
    }

    pub fn write_arm9<T: Into<u32>>(&self, addr_offset: u32, value: T) -> WriteBack {
        match addr_offset {
            0x247 => WriteBack::Byte(self.memory.borrow_mut().set_wram_cnt(value.into() as u8)),
            0x304 => WriteBack::Half(
                self.gpu_context
                    .borrow_mut()
                    .set_pow_cnt1(value.into() as u16),
            ),
            _ => self.write_common(CpuType::ARM9, addr_offset, value),
        }
    }
}

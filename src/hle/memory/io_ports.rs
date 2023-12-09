use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::memory::indirect_memory::indirect_mem_handler::WriteBack;
use crate::hle::memory::memory::Memory;
use crate::hle::registers::ThreadRegs;
use std::cell::RefCell;
use std::rc::Rc;

pub struct IoPorts {
    memory: Rc<RefCell<Memory>>,
    thread_regs: Rc<RefCell<ThreadRegs>>,
    gpu_context: Rc<RefCell<GpuContext>>,
}

impl IoPorts {
    pub fn new(
        memory: Rc<RefCell<Memory>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        gpu_context: Rc<RefCell<GpuContext>>,
    ) -> Self {
        IoPorts {
            memory,
            thread_regs,
            gpu_context,
        }
    }

    pub fn write_arm9<T: Into<u32>>(&self, addr_offset: u32, value: T) -> WriteBack {
        let value = value.into();
        match addr_offset {
            0x180 => todo!(),
            0x247 => WriteBack::Byte(self.memory.borrow_mut().set_wram_cnt(value as u8)),
            0x208 => WriteBack::Byte(self.thread_regs.borrow_mut().set_ime(value as u8)),
            0x300 => WriteBack::Byte(self.thread_regs.borrow_mut().set_post_flg(value as u8)),
            0x304 => WriteBack::Half(self.gpu_context.borrow_mut().set_pow_cnt1(value as u16)),
            _ => todo!("Unimplemented io port {:x}", addr_offset),
        }
    }
}

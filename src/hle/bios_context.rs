use crate::hle::bios_lookup_table::ARM9_SWI_LOOKUP_TABLE;
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::logging::debug_println;
use crate::utils::FastCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct BiosContext {
    regs: Rc<FastCell<ThreadRegs>>,
    mem_handler: Arc<MemHandler>,
}

mod swi {
    use crate::hle::bios_context::BiosContext;
    use crate::jit::reg::Reg;
    use crate::utils;

    pub fn bit_unpack(context: &BiosContext) {
        todo!()
    }

    pub fn cpu_fast_set(context: &BiosContext) {
        todo!()
    }

    pub fn cpu_set(context: &BiosContext) {
        todo!()
    }

    pub fn diff_unfilt16(context: &BiosContext) {
        todo!()
    }

    pub fn diff_unfilt8(context: &BiosContext) {
        todo!()
    }

    pub fn divide(context: &BiosContext) {
        todo!()
    }

    pub fn get_crc16(context: &BiosContext) {
        let (initial, addr, len) = {
            let regs = context.regs.borrow();
            (
                *regs.get_reg_value(Reg::R0),
                *regs.get_reg_value(Reg::R1),
                *regs.get_reg_value(Reg::R2),
            )
        };

        let mut buf = vec![0u8; len as usize];
        context.mem_handler.read_slice(addr, &mut buf);
        let ret = utils::crc16(initial, &buf, 0, len as usize);
        *context.regs.borrow_mut().get_reg_value_mut(Reg::R0) = ret as u32;
    }

    pub fn halt(context: &BiosContext) {
        todo!()
    }

    pub fn huff_uncomp(context: &BiosContext) {
        todo!()
    }

    pub fn interrupt_wait(context: &BiosContext) {
        todo!()
    }

    pub fn is_debugger(context: &BiosContext) {
        *context.regs.borrow_mut().get_reg_value_mut(Reg::R0) = 0;
    }

    pub fn lz77_uncomp(context: &BiosContext) {
        todo!()
    }

    pub fn runlen_uncomp(context: &BiosContext) {
        todo!()
    }

    pub fn square_root(context: &BiosContext) {
        todo!()
    }

    pub fn unknown(context: &BiosContext) {
        todo!()
    }

    pub fn v_blank_intr_wait(context: &BiosContext) {
        todo!()
    }

    pub fn wait_by_loop(context: &BiosContext) {
        todo!()
    }

    pub fn sleep(context: &BiosContext) {
        todo!()
    }

    pub fn sound_bias(context: &BiosContext) {
        todo!()
    }

    pub fn get_sine_table(context: &BiosContext) {
        todo!()
    }

    pub fn get_pitch_table(context: &BiosContext) {
        todo!()
    }

    pub fn get_volume_table(context: &BiosContext) {
        todo!()
    }
}

pub use swi::*;

impl BiosContext {
    pub fn new(regs: Rc<FastCell<ThreadRegs>>, mem_handler: Arc<MemHandler>) -> Self {
        BiosContext { regs, mem_handler }
    }

    pub fn swi(&self, comment: u8) {
        let (name, func) = &ARM9_SWI_LOOKUP_TABLE[comment as usize];
        debug_println!("Swi call {:x} {}", comment, name);
        func(self);
    }
}

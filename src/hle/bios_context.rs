use crate::hle::bios_lookup_table::{ARM7_SWI_LOOKUP_TABLE, ARM9_SWI_LOOKUP_TABLE};
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::logging::debug_println;
use crate::utils::FastCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct BiosContext {
    cpu_type: CpuType,
    regs: Rc<FastCell<ThreadRegs>>,
    mem_handler: Arc<MemHandler>,
    pub cycle_correction: u16,
}

mod swi {
    use crate::hle::bios_context::BiosContext;
    use crate::jit::reg::Reg;
    use crate::utils;

    pub fn bit_unpack(context: &mut BiosContext) {
        todo!()
    }

    pub fn cpu_fast_set(context: &mut BiosContext) {
        todo!()
    }

    pub fn cpu_set(context: &mut BiosContext) {
        todo!()
    }

    pub fn diff_unfilt16(context: &mut BiosContext) {
        todo!()
    }

    pub fn diff_unfilt8(context: &mut BiosContext) {
        todo!()
    }

    pub fn divide(context: &mut BiosContext) {
        todo!()
    }

    pub fn get_crc16(context: &mut BiosContext) {
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

    pub fn halt(context: &mut BiosContext) {
        todo!()
    }

    pub fn huff_uncomp(context: &mut BiosContext) {
        todo!()
    }

    pub fn interrupt_wait(context: &mut BiosContext) {
        todo!()
    }

    pub fn is_debugger(context: &mut BiosContext) {
        *context.regs.borrow_mut().get_reg_value_mut(Reg::R0) = 0;
    }

    pub fn lz77_uncomp(context: &mut BiosContext) {
        todo!()
    }

    pub fn runlen_uncomp(context: &mut BiosContext) {
        todo!()
    }

    pub fn square_root(context: &mut BiosContext) {
        todo!()
    }

    pub fn unknown(context: &mut BiosContext) {
        todo!()
    }

    pub fn v_blank_intr_wait(context: &mut BiosContext) {
        todo!()
    }

    pub fn wait_by_loop(context: &mut BiosContext) {
        let mut regs = context.regs.borrow_mut();
        let delay = regs.get_reg_value_mut(Reg::R0);
        context.cycle_correction = *delay as u16 * 4;
        *delay = 0;
    }

    pub fn sleep(context: &mut BiosContext) {
        todo!()
    }

    pub fn sound_bias(context: &mut BiosContext) {
        todo!()
    }

    pub fn get_sine_table(context: &mut BiosContext) {
        todo!()
    }

    pub fn get_pitch_table(context: &mut BiosContext) {
        todo!()
    }

    pub fn get_volume_table(context: &mut BiosContext) {
        todo!()
    }
}

use crate::hle::CpuType;
pub use swi::*;

impl BiosContext {
    pub fn new(
        cpu_type: CpuType,
        regs: Rc<FastCell<ThreadRegs>>,
        mem_handler: Arc<MemHandler>,
    ) -> Self {
        BiosContext {
            cpu_type,
            regs,
            mem_handler,
            cycle_correction: 0,
        }
    }

    pub fn swi(&mut self, comment: u8) {
        let (name, func) = match self.cpu_type {
            CpuType::ARM9 => &ARM9_SWI_LOOKUP_TABLE[comment as usize],
            CpuType::ARM7 => &ARM7_SWI_LOOKUP_TABLE[comment as usize],
        };
        debug_println!("Swi call {:x} {}", comment, name);
        func(self);
    }
}

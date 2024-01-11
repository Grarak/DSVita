use crate::hle::bios_lookup_table::{ARM7_SWI_LOOKUP_TABLE, ARM9_SWI_LOOKUP_TABLE};
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::logging::debug_println;
use crate::utils::FastCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct BiosContext<const CPU: CpuType> {
    regs: Rc<FastCell<ThreadRegs<CPU>>>,
    mem_handler: Arc<MemHandler<CPU>>,
    cpu_regs: Arc<CpuRegs<CPU>>,
    pub cycle_correction: u16,
}

mod swi {
    use crate::hle::bios_context::BiosContext;
    use crate::hle::CpuType;
    use crate::jit::reg::Reg;
    use crate::utils;

    pub fn bit_unpack<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn cpu_fast_set<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn cpu_set<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn diff_unfilt16<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn diff_unfilt8<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn divide<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn get_crc16<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
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

    pub fn halt<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        context.cpu_regs.halt(0);
    }

    pub fn huff_uncomp<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn interrupt_wait<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn is_debugger<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        *context.regs.borrow_mut().get_reg_value_mut(Reg::R0) = 0;
    }

    pub fn lz77_uncomp<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn runlen_uncomp<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn square_root<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn unknown<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn v_blank_intr_wait<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn wait_by_loop<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        let mut regs = context.regs.borrow_mut();
        let delay = regs.get_reg_value_mut(Reg::R0);
        context.cycle_correction = *delay as u16 * 4;
        *delay = 0;
    }

    pub fn sleep<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn sound_bias<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn get_sine_table<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn get_pitch_table<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn get_volume_table<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }
}

use crate::hle::cpu_regs::CpuRegs;
use crate::hle::CpuType;
pub(super) use swi::*;

impl<const CPU: CpuType> BiosContext<CPU> {
    pub fn new(
        regs: Rc<FastCell<ThreadRegs<CPU>>>,
        cpu_regs: Arc<CpuRegs<CPU>>,
        mem_handler: Arc<MemHandler<CPU>>,
    ) -> Self {
        BiosContext {
            regs,
            cpu_regs,
            mem_handler,
            cycle_correction: 0,
        }
    }
}

impl BiosContext<{ CpuType::ARM9 }> {
    pub fn swi_arm9(&mut self, comment: u8) {
        let (name, func) = &ARM9_SWI_LOOKUP_TABLE[comment as usize];
        debug_println!("Swi call {:x} {}", comment, name);
        func(self);
    }
}

impl BiosContext<{ CpuType::ARM7 }> {
    pub fn swi_arm7(&mut self, comment: u8) {
        let (name, func) = &ARM7_SWI_LOOKUP_TABLE[comment as usize];
        debug_println!("Swi call {:x} {}", comment, name);
        func(self);
    }
}

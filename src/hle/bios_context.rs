use crate::hle::bios_lookup_table::{ARM7_SWI_LOOKUP_TABLE, ARM9_SWI_LOOKUP_TABLE};
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::{Cpsr, ThreadRegs};
use crate::logging::debug_println;
use bilge::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct BiosContext<const CPU: CpuType> {
    regs: Rc<RefCell<ThreadRegs<CPU>>>,
    mem_handler: Rc<MemHandler<CPU>>,
    cpu_regs: Rc<CpuRegs<CPU>>,
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
        buf.iter_mut().enumerate().for_each(|(index, value)| {
            *value = context.mem_handler.read(addr + index as u32);
        });
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
        context.cycle_correction += (*delay as u16) << 2;
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
use crate::jit::reg::Reg;
pub(super) use swi::*;

impl<const CPU: CpuType> BiosContext<CPU> {
    pub fn new(
        regs: Rc<RefCell<ThreadRegs<CPU>>>,
        cpu_regs: Rc<CpuRegs<CPU>>,
        mem_handler: Rc<MemHandler<CPU>>,
    ) -> Self {
        BiosContext {
            regs,
            cpu_regs,
            mem_handler,
            cycle_correction: 0,
        }
    }

    pub fn swi(&mut self, comment: u8) {
        match CPU {
            CpuType::ARM9 => {
                let bios_context = self as *const _ as *mut BiosContext<{ CpuType::ARM9 }>;
                unsafe { bios_context.as_mut() }.unwrap().swi_arm9(comment);
            }
            CpuType::ARM7 => {
                let bios_context = self as *const _ as *mut BiosContext<{ CpuType::ARM7 }>;
                unsafe { bios_context.as_mut() }.unwrap().swi_arm7(comment);
            }
        }
    }

    pub fn interrupt(&self, dtcm_addr: Option<u32>) {
        debug_println!("{:?} interrupt", CPU);

        let mut thread_regs = self.regs.borrow_mut();
        let mut cpsr = Cpsr::from(thread_regs.cpsr);
        cpsr.set_irq_disable(u1::new(1));
        cpsr.set_thumb(u1::new(0));
        cpsr.set_mode(u5::new(0x12));
        thread_regs.set_cpsr::<true>(u32::from(cpsr));

        thread_regs.sp -= 4;
        self.mem_handler.write(thread_regs.sp, thread_regs.pc); // Just save pc instead of calculating LR
        for reg in [Reg::R12, Reg::R3, Reg::R2, Reg::R1, Reg::R0] {
            thread_regs.sp -= 4;
            self.mem_handler
                .write(thread_regs.sp, *thread_regs.get_reg_value(reg));
        }

        match CPU {
            CpuType::ARM9 => {
                thread_regs.lr = 0xFFFF0000;
                thread_regs.pc = self.mem_handler.read(dtcm_addr.unwrap() + 0x3FFC);
            }
            CpuType::ARM7 => {
                thread_regs.lr = 0x00000000;
                thread_regs.pc = self.mem_handler.read(0x3FFFFFC);
            }
        }
    }

    pub fn uninterrupt(&self) {
        debug_println!("{:?} uninterrupt", CPU);

        let mut thread_regs = self.regs.borrow_mut();

        for reg in [Reg::R0, Reg::R1, Reg::R2, Reg::R3, Reg::R12, Reg::LR] {
            *thread_regs.get_reg_value_mut(reg) = self.mem_handler.read(thread_regs.sp);
            thread_regs.sp += 4;
        }
        thread_regs.pc = thread_regs.lr;

        let spsr = thread_regs.spsr;
        thread_regs.set_cpsr::<false>(spsr);
    }
}

impl BiosContext<{ CpuType::ARM9 }> {
    fn swi_arm9(&mut self, comment: u8) {
        let (name, func) = &ARM9_SWI_LOOKUP_TABLE[comment as usize];
        debug_println!("{:?} swi call {:x} {}", CpuType::ARM9, comment, name);
        func(self);
    }
}

impl BiosContext<{ CpuType::ARM7 }> {
    fn swi_arm7(&mut self, comment: u8) {
        let (name, func) = &ARM7_SWI_LOOKUP_TABLE[comment as usize];
        debug_println!("{:?} swi call {:x} {}", CpuType::ARM7, comment, name);
        func(self);
    }
}

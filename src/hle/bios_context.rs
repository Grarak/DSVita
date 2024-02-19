use crate::hle::bios_lookup_table::{ARM7_SWI_LOOKUP_TABLE, ARM9_SWI_LOOKUP_TABLE};
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::{Cpsr, ThreadRegs};
use crate::logging::debug_println;
use bilge::prelude::*;
use std::cell::RefCell;
use std::cmp::min;
use std::mem;
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
        let thread_regs = context.regs.borrow();
        let src_addr = *thread_regs.get_reg_value(Reg::R0);
        let dst_addr = *thread_regs.get_reg_value(Reg::R1);
        let len_mode = *thread_regs.get_reg_value(Reg::R2);

        let count = len_mode & 0xFFFFF;
        let fill = (len_mode & (1 << 24)) != 0;
        let is_32_bit = (len_mode & (1 << 26)) != 0;

        if is_32_bit {
            for i in 0..count {
                let addr = src_addr + if fill { 0 } else { i << 2 };
                let value = context.mem_handler.read::<u32>(addr);
                context.mem_handler.write(dst_addr + (i << 2), value);
            }
        } else {
            for i in 0..count {
                let addr = src_addr + if fill { 0 } else { i << 1 };
                let value = context.mem_handler.read::<u16>(addr);
                context.mem_handler.write(dst_addr + (i << 1), value);
            }
        }
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

    pub fn halt<const CPU: CpuType>(_: &mut BiosContext<CPU>) {
        panic!("{:?} swi halt shouldn't be used", CPU);
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
        let thread_regs = context.regs.borrow();
        let src_addr = *thread_regs.get_reg_value(Reg::R0);
        let dst_addr = *thread_regs.get_reg_value(Reg::R1);

        let size = context.mem_handler.read::<u32>(src_addr) >> 8;
        let mut src = 4;
        let mut dst = 0;

        loop {
            let mut flags = context.mem_handler.read::<u8>(src_addr + src) as u16;
            src += 1;
            for _ in 0..8 {
                if dst >= size {
                    return;
                }

                flags <<= 1;
                if flags & (1 << 8) != 0 {
                    let val1 = context.mem_handler.read::<u8>(src_addr + src);
                    src += 1;
                    let val2 = context.mem_handler.read::<u8>(src_addr + src);
                    src += 1;
                    let size = 3 + ((val1 >> 4) & 0xF);
                    let offset = 1 + ((val1 as u32 & 0xF) << 8) + val2 as u32;

                    for _ in 0..size {
                        let value = context.mem_handler.read::<u8>(dst_addr + dst - offset);
                        context.mem_handler.write(dst_addr + dst, value);
                        dst += 1;
                    }
                } else {
                    let value = context.mem_handler.read::<u8>(src_addr + src);
                    src += 1;
                    context.mem_handler.write(dst_addr + dst, value);
                    dst += 1;
                }
            }
        }
    }

    pub fn runlen_uncomp<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn square_root<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        todo!()
    }

    pub fn unknown<const CPU: CpuType>(context: &mut BiosContext<CPU>) {}

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
        context.cpu_regs.set_halt_cnt(0xC0);
    }

    pub fn sound_bias<const CPU: CpuType>(context: &mut BiosContext<CPU>) {
        let thread_regs = context.regs.borrow();
        let bias_level = if *thread_regs.get_reg_value(Reg::R0) != 0 {
            0x200u16
        } else {
            0u16
        };
        context.mem_handler.write(0x4000504, bias_level);
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
                let context: &mut BiosContext<{ CpuType::ARM9 }> = unsafe { mem::transmute(self) };
                context.swi_arm9(comment);
            }
            CpuType::ARM7 => {
                let context: &mut BiosContext<{ CpuType::ARM7 }> = unsafe { mem::transmute(self) };
                context.swi_arm7(comment);
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

        let is_thumb = (thread_regs.pc & 1) == 1;
        let mut spsr = Cpsr::from(thread_regs.spsr);
        spsr.set_thumb(u1::from(is_thumb));
        thread_regs.spsr = u32::from(spsr);

        thread_regs.sp -= 4;
        self.mem_handler.write(thread_regs.sp, thread_regs.pc + 4);
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
        thread_regs.pc = thread_regs.lr - 4;

        let spsr = thread_regs.spsr;
        if bool::from(Cpsr::from(spsr).thumb()) {
            thread_regs.pc |= 1;
        } else {
            thread_regs.pc &= !1;
        }
        thread_regs.set_cpsr::<false>(spsr);
    }
}

impl BiosContext<{ CpuType::ARM9 }> {
    fn swi_arm9(&mut self, comment: u8) {
        let (name, func) =
            &ARM9_SWI_LOOKUP_TABLE[min(comment as usize, ARM9_SWI_LOOKUP_TABLE.len() - 1)];
        debug_println!("{:?} swi call {:x} {}", CpuType::ARM9, comment, name);
        func(self);
    }
}

impl BiosContext<{ CpuType::ARM7 }> {
    fn swi_arm7(&mut self, comment: u8) {
        let (name, func) =
            &ARM7_SWI_LOOKUP_TABLE[min(comment as usize, ARM7_SWI_LOOKUP_TABLE.len() - 1)];
        debug_println!("{:?} swi call {:x} {}", CpuType::ARM7, comment, name);
        func(self);
    }
}

pub unsafe extern "C" fn bios_uninterrupt<const CPU: CpuType>(
    bios_context: *const BiosContext<CPU>,
) {
    (*bios_context).uninterrupt();
}

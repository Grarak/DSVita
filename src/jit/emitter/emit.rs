use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::branch_assembler::{Bx, B};
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, Mrs};
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{Reg, RegReserve, EMULATED_REGS_COUNT, FIRST_EMULATED_REG};
use crate::jit::{Cond, Op};
use std::cmp::max;
use std::{ops, ptr};

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];
        let cond = inst_info.cond;
        let out_regs = inst_info.out_regs;

        let emit_func = match inst_info.op {
            Op::B | Op::Bl => JitAsm::emit_b,
            Op::Bx | Op::BlxReg => JitAsm::emit_bx,
            Op::LdrOfim | Op::LdrOfip | Op::LdrbOfrplr | Op::LdrPtip => JitAsm::emit_ldr,
            Op::LdmiaW => JitAsm::emit_ldm,
            Op::StrOfip | Op::StrbOfip | Op::StrhOfip | Op::StrPrim => JitAsm::emit_str,
            Op::Stmia | Op::Stmdb | Op::StmiaW | Op::StmdbW => JitAsm::emit_stm,
            Op::Mcr | Op::Mrc => JitAsm::emit_cp15,
            Op::MsrRc | Op::MsrIc => JitAsm::emit_msr_cprs,
            Op::MrsRc => JitAsm::emit_mrs_cprs,
            Op::Swi => JitAsm::emit_swi,
            _ => {
                let src_regs = inst_info.src_regs;
                let combined_regs = src_regs + out_regs;
                if combined_regs.emulated_regs_count() > 0 {
                    self.handle_emulated_regs(buf_index, pc, |_, _, _| Vec::new());
                } else {
                    self.jit_buf.emit_opcodes.push(inst_info.opcode);
                }

                if out_regs.is_reserved(Reg::CPSR) {
                    let mut reserved = combined_regs
                        .create_push_pop_handler(2, &self.jit_buf.instructions[buf_index + 1..]);

                    let host_cpsr_reg = reserved.pop().unwrap();
                    let guest_cpsr_reg = reserved.pop().unwrap();

                    if let Some(opcode) = reserved.emit_push_stack(Reg::LR) {
                        self.jit_buf.emit_opcodes.push(opcode)
                    }

                    self.handle_cpsr(host_cpsr_reg, guest_cpsr_reg);

                    if let Some(opcode) = reserved.emit_pop_stack(Reg::LR) {
                        self.jit_buf.emit_opcodes.push(opcode)
                    }
                }

                |_: &mut JitAsm<CPU>, _: usize, _: u32| {}
            }
        };

        emit_func(self, buf_index, pc);

        if out_regs.is_reserved(Reg::PC) {
            if cond != Cond::AL {
                todo!()
            }

            self.jit_buf
                .emit_opcodes
                .extend(&self.thread_regs.borrow().save_regs_opcodes);

            self.jit_buf
                .emit_opcodes
                .extend(&AluImm::mov32(Reg::R0, pc));
            self.jit_buf.emit_opcodes.extend(AluImm::mov32(
                Reg::LR,
                ptr::addr_of_mut!(self.guest_branch_out_pc) as u32,
            ));
            self.jit_buf
                .emit_opcodes
                .push(LdrStrImm::str_al(Reg::R0, Reg::LR));

            Self::emit_host_bx(
                self.breakout_skip_save_regs_addr,
                &mut self.jit_buf.emit_opcodes,
            );
        }
    }

    pub fn handle_emulated_regs(
        &mut self,
        buf_index: usize,
        pc: u32,
        before_assemble: fn(
            &JitAsm<CPU>,
            inst_info: &InstInfo,
            reg_reserver: &mut RegPushPopHandler,
        ) -> Vec<u32>,
    ) {
        let mut inst_info = self.jit_buf.instructions[buf_index].clone();

        let mut opcodes = Vec::<u32>::new();

        let emulated_src_regs = inst_info.src_regs.get_emulated_regs();
        let mut src_reserved = inst_info.src_regs.create_push_pop_handler(
            emulated_src_regs.len() as u8,
            &self.jit_buf.instructions[buf_index + 1..],
        );
        src_reserved.add_reg_reserve(inst_info.out_regs);
        src_reserved.use_gp();

        let mut src_reg_mapping: [Reg; EMULATED_REGS_COUNT] = [Reg::None; EMULATED_REGS_COUNT];

        let mut handle_src_reg = |reg: &mut Reg| {
            let mapped_reg = &mut src_reg_mapping[(*reg as u8 - FIRST_EMULATED_REG as u8) as usize];
            if *mapped_reg == Reg::None {
                *mapped_reg = src_reserved.pop().unwrap();
            }

            *reg = *mapped_reg;
        };

        for operand in inst_info.operands_mut() {
            match operand {
                Operand::Reg { reg, shift } => {
                    if let Some(shift) = shift {
                        match match shift {
                            Shift::LSL(v) => v,
                            Shift::LSR(v) => v,
                            Shift::ASR(v) => v,
                            Shift::ROR(v) => v,
                        } {
                            ShiftValue::Reg(shift_reg) => {
                                if emulated_src_regs.is_reserved(*shift_reg) {
                                    handle_src_reg(shift_reg)
                                }
                            }
                            ShiftValue::Imm(_) => {}
                        }
                    }

                    if emulated_src_regs.is_reserved(*reg) {
                        handle_src_reg(reg);
                    }
                }
                _ => {}
            }
        }

        if let Some(opcode) = src_reserved.emit_push_stack(Reg::LR) {
            opcodes.push(opcode);
        }

        for (index, mapped_reg) in src_reg_mapping.iter().enumerate() {
            if *mapped_reg == Reg::None {
                continue;
            }

            let reg = Reg::from(FIRST_EMULATED_REG as u8 + index as u8);
            if reg == Reg::PC {
                opcodes.extend(&AluImm::mov32(*mapped_reg, pc + 8));
            } else {
                opcodes.extend(self.thread_regs.borrow().emit_get_reg(*mapped_reg, reg));
            }
        }

        let emulated_out_regs = inst_info.out_regs.get_emulated_regs();

        let mut out_reserved: RegReserve = src_reg_mapping.iter().sum();
        out_reserved = out_reserved.get_gp_regs();
        out_reserved += inst_info.out_regs.get_gp_regs();

        let num_out_reserve = max(
            ((emulated_out_regs.len() + 1) as i32) - out_reserved.len() as i32,
            0,
        );

        let mut out_reserved = RegPushPopHandler::from(out_reserved)
            + RegPushPopHandler::from(out_reserved.get_writable_gp_regs(
                num_out_reserve as u8,
                &self.jit_buf.instructions[buf_index + 1..],
            ));
        out_reserved.use_gp();

        let out_addr = out_reserved.pop().unwrap();
        let mut out_reg_mapping: [Reg; EMULATED_REGS_COUNT] = [Reg::None; EMULATED_REGS_COUNT];

        for operand in inst_info.operands_mut() {
            if let Operand::Reg { reg, .. } = operand {
                if emulated_out_regs.is_reserved(*reg) {
                    let mapped_reg =
                        &mut out_reg_mapping[(*reg as u8 - FIRST_EMULATED_REG as u8) as usize];
                    if *mapped_reg == Reg::None {
                        *mapped_reg = out_reserved.pop().unwrap();
                    }

                    *reg = *mapped_reg;
                }
            }
        }

        let insts = before_assemble(self, &inst_info, &mut out_reserved);
        {
            if let Some(opcode) = out_reserved.emit_push_stack(Reg::LR) {
                opcodes.push(opcode);
            }
        }
        opcodes.extend(&insts);
        opcodes.push(inst_info.assemble());

        for (index, mapped_reg) in out_reg_mapping.iter().enumerate() {
            if *mapped_reg == Reg::None {
                continue;
            }

            let reg = Reg::from(FIRST_EMULATED_REG as u8 + index as u8);
            if reg == Reg::PC {
                todo!()
            } else {
                opcodes.extend(
                    self.thread_regs
                        .borrow()
                        .emit_set_reg(reg, *mapped_reg, out_addr),
                );
            }
        }

        if let Some(opcode) = out_reserved.emit_pop_stack(Reg::LR) {
            opcodes.push(opcode);
        }

        if let Some(opcode) = src_reserved.emit_pop_stack(Reg::LR) {
            opcodes.push(opcode);
        }

        if inst_info.cond != Cond::AL {
            self.jit_buf
                .emit_opcodes
                .push(B::b(opcodes.len() as i32 - 1, !inst_info.cond));
        }

        self.jit_buf.emit_opcodes.extend(&opcodes);
    }

    pub fn emit_host_bx(addr: u32, jit_buf: &mut Vec<u32>) {
        jit_buf.extend(&AluImm::mov32(Reg::LR, addr));
        jit_buf.push(Bx::bx(Reg::LR, Cond::AL));
    }

    pub fn emit_call_host_func<F: FnOnce(&mut JitAsm<CPU>)>(
        &mut self,
        after_host_restore: F,
        args: &[Option<u32>],
        func_addr: *const (),
    ) {
        let thumb = self.thread_regs.borrow().is_thumb();
        self.jit_buf.emit_opcodes.extend(if thumb {
            &self.restore_host_thumb_opcodes
        } else {
            &self.restore_host_opcodes
        });

        if args.len() > 4 {
            todo!()
        }

        after_host_restore(self);

        for (index, arg) in args.iter().enumerate() {
            if let Some(arg) = arg {
                self.jit_buf
                    .emit_opcodes
                    .extend(AluImm::mov32(Reg::from(index as u8), *arg));
            }
        }

        self.jit_buf
            .emit_opcodes
            .extend(&AluImm::mov32(Reg::LR, func_addr as u32));
        self.jit_buf.emit_opcodes.push(Bx::blx(Reg::LR, Cond::AL));

        self.jit_buf.emit_opcodes.extend(if thumb {
            &self.restore_guest_thumb_opcodes
        } else {
            &self.restore_guest_opcodes
        });
    }

    pub fn handle_cpsr(&mut self, host_cpsr_reg: Reg, guest_cpsr_reg: Reg) {
        self.jit_buf
            .emit_opcodes
            .push(Mrs::cpsr(host_cpsr_reg, Cond::AL));
        self.jit_buf.emit_opcodes.extend(
            &self
                .thread_regs
                .borrow()
                .emit_get_reg(guest_cpsr_reg, Reg::CPSR),
        );

        // Only copy the cond flags from host cpsr
        self.jit_buf.emit_opcodes.push(AluImm::and(
            host_cpsr_reg,
            host_cpsr_reg,
            0xF8,
            4, // 8 Bytes, steps of 2
            Cond::AL,
        ));
        self.jit_buf.emit_opcodes.push(AluImm::bic(
            guest_cpsr_reg,
            guest_cpsr_reg,
            0xF8,
            4, // 8 Bytes, steps of 2
            Cond::AL,
        ));
        self.jit_buf.emit_opcodes.push(AluShiftImm::orr_al(
            guest_cpsr_reg,
            host_cpsr_reg,
            guest_cpsr_reg,
        ));
        self.jit_buf
            .emit_opcodes
            .extend(self.thread_regs.borrow().emit_set_reg(
                Reg::CPSR,
                guest_cpsr_reg,
                host_cpsr_reg,
            ));
    }
}

impl RegReserve {
    pub fn create_push_pop_handler(&self, num: u8, insts: &[InstInfo]) -> RegPushPopHandler {
        let mut handler = RegPushPopHandler::from(self.get_writable_gp_regs(num, insts));
        handler.use_gp();
        handler
    }

    pub fn get_writable_gp_regs(&self, num: u8, insts: &[InstInfo]) -> RegReserve {
        let mut reserve = *self;
        let mut writable_regs = RegReserve::new();
        for inst in insts {
            if inst.op.is_branch()
                || inst.op.is_branch_thumb()
                || inst.out_regs.is_reserved(Reg::PC)
            {
                break;
            }

            reserve += inst.src_regs;
            let writable = (reserve & inst.out_regs) ^ inst.out_regs;
            writable_regs += writable;

            let gp = writable_regs.get_gp_regs();
            if gp.len() >= num as usize {
                return gp;
            }
        }
        writable_regs.get_gp_regs()
    }
}

pub struct RegPushPopHandler {
    reg_reserve: RegReserve,
    not_reserved: RegReserve,
    regs_to_save: RegReserve,
    regs_to_skip: RegReserve,
    pushed: bool,
}

impl From<RegReserve> for RegPushPopHandler {
    fn from(value: RegReserve) -> Self {
        RegPushPopHandler {
            reg_reserve: value,
            not_reserved: !value,
            regs_to_save: RegReserve::new(),
            regs_to_skip: RegReserve::new(),
            pushed: false,
        }
    }
}

impl RegPushPopHandler {
    pub fn set_regs_to_skip(&mut self, regs_to_skip: RegReserve) {
        self.reg_reserve -= regs_to_skip;
        self.not_reserved -= regs_to_skip;
        self.regs_to_skip = regs_to_skip;
    }

    pub fn use_gp(&mut self) {
        self.reg_reserve = self.reg_reserve.get_gp_regs();
        self.not_reserved = self.not_reserved.get_gp_regs();
        self.regs_to_save = self.regs_to_save.get_gp_regs();
    }

    pub fn add_reg_reserve(&mut self, reg_reserve: RegReserve) {
        self.reg_reserve += reg_reserve;
    }

    pub fn pop(&mut self) -> Option<Reg> {
        debug_assert!(!self.pushed);
        match self.reg_reserve.pop() {
            Some(reg) => Some(reg),
            None => {
                let reg = self.not_reserved.pop();
                if let Some(reg) = reg {
                    self.regs_to_save += reg
                }
                reg
            }
        }
    }

    pub fn emit_push_stack(&mut self, sp: Reg) -> Option<u32> {
        debug_assert!(!self.pushed);
        self.pushed = true;
        if self.regs_to_save.len() == 0 {
            None
        } else {
            Some(LdmStm::push_pre(self.regs_to_save, sp, Cond::AL))
        }
    }

    pub fn emit_pop_stack(self, sp: Reg) -> Option<u32> {
        debug_assert!(self.pushed);
        if self.regs_to_save.len() == 0 {
            None
        } else {
            Some(LdmStm::pop_post(self.regs_to_save, sp, Cond::AL))
        }
    }
}

impl ops::Add<RegPushPopHandler> for RegPushPopHandler {
    type Output = RegPushPopHandler;

    fn add(self, rhs: RegPushPopHandler) -> Self::Output {
        let reg_reserve = self.reg_reserve + rhs.reg_reserve;
        let mut instance = RegPushPopHandler {
            reg_reserve,
            not_reserved: !reg_reserve,
            regs_to_save: RegReserve::new(),
            regs_to_skip: RegReserve::new(),
            pushed: false,
        };
        instance.set_regs_to_skip(self.regs_to_skip + rhs.regs_to_skip);
        instance
    }
}

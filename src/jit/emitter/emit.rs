use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::assembler::arm::transfer_assembler::LdmStm;
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::jit::JitAsm;
use crate::jit::reg::{Reg, RegReserve, EMULATED_REGS_COUNT, FIRST_EMULATED_REG};
use crate::jit::{Cond, Op};
use std::cmp::max;
use std::ops;

impl JitAsm {
    pub fn emit(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.opcode_buf[buf_index];

        let emit_func = match inst_info.op {
            Op::B => JitAsm::emit_b,
            Op::BlxReg => JitAsm::emit_blx,
            Op::Mcr | Op::Mrc => JitAsm::emit_cp15,
            Op::MovAri
            | Op::MovArr
            | Op::MovImm
            | Op::MovLli
            | Op::MovLlr
            | Op::MovLri
            | Op::MovLrr
            | Op::MovRri
            | Op::MovRrr
            | Op::MovsAri
            | Op::MovsArr
            | Op::MovsImm
            | Op::MovsLli
            | Op::MovsLlr
            | Op::MovsLri
            | Op::MovsLrr
            | Op::MovsRri
            | Op::MovsRrr => JitAsm::emit_mov,
            Op::LdrOfip => JitAsm::emit_ldr,
            Op::StrOfip | Op::StrhOfip => JitAsm::emit_str,
            _ => {
                debug_assert_eq!(
                    (inst_info.src_regs + inst_info.out_regs).emulated_regs_count(),
                    0
                );

                self.jit_buf.push(inst_info.opcode);
                |_: &mut JitAsm, _: usize, _: u32| {}
            }
        };

        emit_func(self, buf_index, pc);
    }

    pub fn handle_emulated_regs(
        &mut self,
        buf_index: usize,
        pc: u32,
        before_assemble: fn(
            &JitAsm,
            inst_info: &InstInfo,
            reg_reserver: &mut RegPushPopHandler,
        ) -> Vec<u32>,
    ) {
        let mut inst_info = self.opcode_buf[buf_index];

        let emulated_src_regs = inst_info.src_regs.get_emulated_regs();
        let mut src_reserved = inst_info.src_regs.create_push_pop_handler(
            emulated_src_regs.len() as u8,
            &self.opcode_buf[buf_index + 1..],
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
                        handle_src_reg(reg)
                    }
                }
                _ => {}
            }
        }

        if let Some(opcode) = src_reserved.emit_push_stack(Reg::LR) {
            self.jit_buf.push(opcode);
        }

        for (index, mapped_reg) in src_reg_mapping.iter().enumerate() {
            if *mapped_reg == Reg::None {
                continue;
            }

            let reg = Reg::from(FIRST_EMULATED_REG as u8 + index as u8);
            if reg == Reg::PC {
                self.jit_buf
                    .extend_from_slice(&AluImm::mov32(*mapped_reg, pc + 8));
            } else {
                self.jit_buf
                    .extend_from_slice(&self.thread_regs.borrow().emit_get_reg(*mapped_reg, reg));
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
            + RegPushPopHandler::from(
                out_reserved
                    .get_writable_gp_regs(num_out_reserve as u8, &self.opcode_buf[buf_index + 1..]),
            );
        out_reserved.use_gp();

        let out_addr = out_reserved.pop().unwrap();
        let mut out_reg_mapping: [Reg; EMULATED_REGS_COUNT] = [Reg::None; EMULATED_REGS_COUNT];

        for operand in inst_info.operands_mut() {
            match operand {
                Operand::Reg { reg, .. } => {
                    if emulated_out_regs.is_reserved(*reg) {
                        let mapped_reg =
                            &mut out_reg_mapping[(*reg as u8 - FIRST_EMULATED_REG as u8) as usize];
                        if *mapped_reg == Reg::None {
                            *mapped_reg = out_reserved.pop().unwrap();
                        }

                        *reg = *mapped_reg;
                    }
                }
                _ => {}
            }
        }

        let insts = before_assemble(self, &inst_info, &mut out_reserved);
        {
            if let Some(opcode) = out_reserved.emit_push_stack(Reg::LR) {
                self.jit_buf.push(opcode);
            }
        }
        self.jit_buf.extend_from_slice(&insts);
        self.jit_buf.push(inst_info.assemble());

        for (index, mapped_reg) in out_reg_mapping.iter().enumerate() {
            if *mapped_reg == Reg::None {
                continue;
            }

            let reg = Reg::from(FIRST_EMULATED_REG as u8 + index as u8);
            if reg == Reg::PC {
                todo!()
            } else {
                self.jit_buf
                    .extend_from_slice(&self.thread_regs.borrow().emit_set_reg(
                        reg,
                        *mapped_reg,
                        out_addr,
                    ));
            }
        }

        if let Some(opcode) = out_reserved.emit_pop_stack(Reg::LR) {
            self.jit_buf.push(opcode);
        }

        if let Some(opcode) = src_reserved.emit_pop_stack(Reg::LR) {
            self.jit_buf.push(opcode);
        }
    }

    pub fn emit_host_bx(addr: u32, jit_buf: &mut Vec<u32>) {
        jit_buf.extend_from_slice(&AluImm::mov32(Reg::LR, addr));
        jit_buf.push(Bx::bx(Reg::LR, Cond::AL));
    }
}

impl RegReserve {
    pub fn create_push_pop_handler(&self, num: u8, insts: &[InstInfo]) -> RegPushPopHandler {
        RegPushPopHandler::from(self.get_writable_gp_regs(num, insts))
    }

    pub fn get_writable_gp_regs(&self, num: u8, insts: &[InstInfo]) -> RegReserve {
        let mut reserve = *self;
        let mut writable_regs = RegReserve::new();
        for inst in insts {
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
}

impl From<RegReserve> for RegPushPopHandler {
    fn from(value: RegReserve) -> Self {
        RegPushPopHandler {
            reg_reserve: value,
            not_reserved: !value,
            regs_to_save: RegReserve::new(),
            regs_to_skip: RegReserve::new(),
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

    pub fn emit_push_stack(&self, sp: Reg) -> Option<u32> {
        if self.regs_to_save.len() == 0 {
            None
        } else {
            Some(LdmStm::push_pre(self.regs_to_save, sp, Cond::AL))
        }
    }

    pub fn emit_pop_stack(&self, sp: Reg) -> Option<u32> {
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
        let mut instance =
            RegPushPopHandler {
                reg_reserve,
                not_reserved: !reg_reserve,
                regs_to_save: RegReserve::new(),
                regs_to_skip: RegReserve::new(),
            };
        instance.set_regs_to_skip(self.regs_to_skip + rhs.regs_to_skip);
        instance
    }
}

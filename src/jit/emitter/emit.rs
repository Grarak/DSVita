use crate::hle::hle::{get_cm, get_regs, get_regs_mut};
use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::branch_assembler::{Bx, B};
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, Mrs};
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::inst_threag_regs_handler::register_restore_spsr;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{Reg, RegReserve, EMULATED_REGS_COUNT, FIRST_EMULATED_REG};
use crate::jit::{Cond, Op, ShiftType};
use std::cmp::max;
use std::{ops, ptr};

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];
        let op = inst_info.op;
        let cond = inst_info.cond;
        let out_regs = inst_info.out_regs;

        let emit_func: fn(&mut JitAsm<'a, CPU>, buf_index: usize, pc: u32) = match op {
            Op::B | Op::Bl => Self::emit_b,
            Op::Bx | Op::BlxReg => Self::emit_bx,
            Op::Mcr | Op::Mrc => Self::emit_cp15,
            Op::MsrRc | Op::MsrIc | Op::MsrRs | Op::MsrIs => Self::emit_msr,
            Op::MrsRc | Op::MrsRs => Self::emit_mrs,
            Op::Swi => Self::emit_swi::<false>,
            Op::Swpb | Op::Swp => Self::emit_swp,
            Op::UnkArm => Self::emit_unknown,
            _ => {
                if op.is_single_mem_transfer() {
                    if op.mem_is_write() {
                        Self::emit_str
                    } else {
                        Self::emit_ldr
                    }
                } else if op.is_multiple_mem_transfer() {
                    Self::emit_multiple_transfer::<false>
                } else {
                    let src_regs = inst_info.src_regs;
                    let combined_regs = src_regs + out_regs;
                    if combined_regs.emulated_regs_count() > 0 {
                        self.handle_emulated_regs(buf_index, pc);
                    } else {
                        let mut inst_info = inst_info.clone();
                        inst_info.set_cond(Cond::AL);
                        self.jit_buf.emit_opcodes.push(inst_info.opcode);
                    }

                    if out_regs.is_reserved(Reg::CPSR) {
                        let mut reserved = combined_regs.create_push_pop_handler(
                            2,
                            &self.jit_buf.instructions[buf_index],
                            &self.jit_buf.instructions[buf_index + 1..],
                        );

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
            }
        };

        emit_func(self, buf_index, pc);

        if out_regs.is_reserved(Reg::PC) {
            let opcodes = &mut self.jit_buf.emit_opcodes;
            let restore_spsr = out_regs.is_reserved(Reg::CPSR) && op.is_arm_alu();

            let regs = get_regs_mut!(self.hle, CPU);
            if restore_spsr {
                opcodes.extend(&self.restore_host_opcodes);
                opcodes.extend(AluImm::mov32(Reg::R0, regs as *mut _ as _));
                opcodes.extend(AluImm::mov32(Reg::R1, get_cm!(self.hle) as *const _ as _));
                Self::emit_host_blx(register_restore_spsr as *const () as _, opcodes);
            } else {
                opcodes.extend(&regs.save_regs_opcodes);
            }

            opcodes.extend(&AluImm::mov32(Reg::R0, pc));
            opcodes.extend(AluImm::mov32(
                Reg::R1,
                ptr::addr_of_mut!(self.guest_branch_out_pc) as u32,
            ));

            if CPU == CpuType::ARM7
                || (!op.is_single_mem_transfer() && !op.is_multiple_mem_transfer())
            {
                opcodes.extend(regs.emit_get_reg(Reg::R2, Reg::PC));
                if restore_spsr {
                    opcodes.extend(regs.emit_get_reg(Reg::R3, Reg::CPSR));
                    opcodes.push(AluImm::mov_al(Reg::R4, 1));
                    opcodes.push(AluShiftImm::bic_al(Reg::R2, Reg::R2, Reg::R4));
                    opcodes.push(AluShiftImm::and(
                        Reg::R3,
                        Reg::R4,
                        Reg::R3,
                        ShiftType::Lsr,
                        5,
                        Cond::AL,
                    ));
                    opcodes.push(AluShiftImm::orr_al(Reg::R2, Reg::R2, Reg::R3));
                } else {
                    opcodes.push(AluImm::bic_al(Reg::R2, Reg::R2, 1));
                }
                opcodes.extend(regs.emit_set_reg(Reg::PC, Reg::R2, Reg::R3));
            } else if restore_spsr {
                opcodes.extend(regs.emit_get_reg(Reg::R2, Reg::PC));
                opcodes.extend(regs.emit_get_reg(Reg::R3, Reg::CPSR));
                opcodes.push(AluImm::mov_al(Reg::R4, 1));
                opcodes.push(AluShiftImm::bic_al(Reg::R2, Reg::R2, Reg::R4));
                opcodes.push(AluShiftImm::and(
                    Reg::R3,
                    Reg::R4,
                    Reg::R3,
                    ShiftType::Lsr,
                    5,
                    Cond::AL,
                ));
                opcodes.push(AluShiftImm::orr_al(Reg::R2, Reg::R2, Reg::R3));
                opcodes.extend(regs.emit_set_reg(Reg::PC, Reg::R2, Reg::R3));
            }

            opcodes.push(LdrStrImm::str_al(Reg::R0, Reg::R1));

            Self::emit_host_bx(self.breakout_skip_save_regs_addr, opcodes);
        }

        if cond != Cond::AL {
            if cond != Cond::NV {
                let len = self.jit_buf.emit_opcodes.len();
                if len == 1 {
                    let opcode = &mut self.jit_buf.emit_opcodes[0];
                    *opcode = (*opcode & !(0xF << 28)) | ((cond as u32) << 28);
                } else {
                    self.jit_buf
                        .emit_opcodes
                        .insert(0, B::b(len as i32 - 1, !cond));
                }
            } else {
                self.jit_buf.emit_opcodes.clear();
            }
        }
    }

    pub fn handle_emulated_regs(&mut self, buf_index: usize, pc: u32) {
        let og_inst_info = &self.jit_buf.instructions[buf_index].clone();
        let mut inst_info = og_inst_info.clone();

        let opcodes = &mut self.jit_buf.emit_opcodes;

        let emulated_src_regs = inst_info.src_regs.get_emulated_regs();
        let mut src_reserved = inst_info.src_regs.create_push_pop_handler(
            emulated_src_regs.len() as u8,
            &self.jit_buf.instructions[buf_index],
            &self.jit_buf.instructions[buf_index + 1..],
        );
        src_reserved.set_regs_to_skip(inst_info.src_regs + inst_info.out_regs);
        src_reserved.use_gp();

        let mut reg_mapping: [Reg; EMULATED_REGS_COUNT] = [Reg::None; EMULATED_REGS_COUNT];

        let mut handle_src_reg = |reg: &mut Reg| {
            let mapped_reg = &mut reg_mapping[(*reg as u8 - FIRST_EMULATED_REG as u8) as usize];
            if *mapped_reg == Reg::None {
                *mapped_reg = src_reserved.pop().unwrap();
            }

            *reg = *mapped_reg;
        };

        for operand in inst_info.operands_mut() {
            if let Operand::Reg { reg, shift } = operand {
                if let Some(shift) = shift {
                    match match shift {
                        Shift::Lsl(v) => v,
                        Shift::Lsr(v) => v,
                        Shift::Asr(v) => v,
                        Shift::Ror(v) => v,
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
        }

        if let Some(opcode) = src_reserved.emit_push_stack(Reg::LR) {
            opcodes.push(opcode);
        }

        for (index, mapped_reg) in reg_mapping.iter().enumerate() {
            if *mapped_reg == Reg::None {
                continue;
            }

            let reg = Reg::from(FIRST_EMULATED_REG as u8 + index as u8);
            if reg == Reg::PC {
                if inst_info.op.is_alu_reg_shift()
                    && *og_inst_info.operands().last().unwrap().as_reg().unwrap().0 == Reg::PC
                {
                    opcodes.extend(&AluImm::mov32(*mapped_reg, pc + 12));
                } else {
                    opcodes.extend(&AluImm::mov32(*mapped_reg, pc + 8));
                }
            } else {
                opcodes.extend(get_regs!(self.hle, CPU).emit_get_reg(*mapped_reg, reg));
            }
        }

        let emulated_out_regs = inst_info.out_regs.get_emulated_regs();

        let mut out_used_regs: RegReserve = reg_mapping.iter().sum();
        out_used_regs = out_used_regs.get_gp_regs();
        out_used_regs += inst_info.out_regs.get_gp_regs();

        let num_out_reserve = max(
            ((emulated_out_regs.len() + 1) as i32) - out_used_regs.len() as i32,
            0,
        );

        let mut out_reserved = RegPushPopHandler::from(out_used_regs.get_writable_gp_regs(
            num_out_reserve as u8,
            &self.jit_buf.instructions[buf_index],
            &self.jit_buf.instructions[buf_index + 1..],
        ));
        out_reserved.set_regs_to_skip(out_used_regs);
        out_reserved.set_regs_to_skip(inst_info.src_regs + inst_info.out_regs);
        out_reserved.use_gp();

        for operand in inst_info.operands_mut() {
            if let Operand::Reg { reg, .. } = operand {
                if emulated_out_regs.is_reserved(*reg) {
                    let mapped_reg =
                        &mut reg_mapping[(*reg as u8 - FIRST_EMULATED_REG as u8) as usize];
                    if *mapped_reg == Reg::None {
                        *mapped_reg = out_reserved.pop().unwrap();
                    }

                    *reg = *mapped_reg;
                }
            }
        }

        inst_info.set_cond(Cond::AL);
        {
            let mut out_addr = None;
            let mut opcodes_after_save = Vec::new();
            opcodes_after_save.push(inst_info.assemble());

            for reg in emulated_out_regs {
                let mapped_reg = reg_mapping[reg as usize - FIRST_EMULATED_REG as usize];
                if mapped_reg == Reg::None {
                    continue;
                }

                if out_addr.is_none() {
                    out_addr = Some(out_reserved.pop().unwrap());
                }
                opcodes_after_save.extend(get_regs!(self.hle, CPU).emit_set_reg(
                    reg,
                    mapped_reg,
                    out_addr.unwrap(),
                ));
            }

            if let Some(opcode) = out_reserved.emit_push_stack(Reg::LR) {
                opcodes.push(opcode);
            }
            opcodes.extend(opcodes_after_save);
        }

        if let Some(opcode) = out_reserved.emit_pop_stack(Reg::LR) {
            opcodes.push(opcode);
        }

        if let Some(opcode) = src_reserved.emit_pop_stack(Reg::LR) {
            opcodes.push(opcode);
        }
    }

    pub fn emit_host_bx(addr: u32, jit_buf: &mut Vec<u32>) {
        jit_buf.extend(AluImm::mov32(Reg::LR, addr));
        jit_buf.push(Bx::bx(Reg::LR, Cond::AL));
    }

    pub fn emit_host_blx(addr: u32, jit_buf: &mut Vec<u32>) {
        jit_buf.extend(AluImm::mov32(Reg::LR, addr));
        jit_buf.push(Bx::blx(Reg::LR, Cond::AL));
    }

    pub fn emit_call_host_func<R, F: FnOnce(&mut Self) -> R, F1>(
        &mut self,
        after_host_restore: F,
        before_guest_restore: F1,
        args: &[Option<u32>],
        func_addr: *const (),
    ) where
        F1: FnOnce(&mut Self, R),
    {
        let thumb = get_regs!(self.hle, CPU).is_thumb();
        if self.jit_buf.regs_saved_previously {
            self.jit_buf
                .emit_opcodes
                .extend(&self.restore_host_no_save_opcodes);
        } else {
            self.jit_buf.emit_opcodes.extend(if thumb {
                &self.restore_host_thumb_opcodes
            } else {
                &self.restore_host_opcodes
            });
        }

        if args.len() > 4 {
            todo!()
        }

        let arg = after_host_restore(self);

        for (index, arg) in args.iter().enumerate() {
            if let Some(arg) = arg {
                self.jit_buf
                    .emit_opcodes
                    .extend(AluImm::mov32(Reg::from(index as u8), *arg));
            }
        }

        Self::emit_host_blx(func_addr as u32, &mut self.jit_buf.emit_opcodes);

        before_guest_restore(self, arg);

        self.jit_buf.emit_opcodes.extend(if thumb {
            &self.restore_guest_thumb_opcodes
        } else {
            &self.restore_guest_opcodes
        });

        self.jit_buf.regs_saved = true;
    }

    pub fn handle_cpsr(&mut self, host_cpsr_reg: Reg, guest_cpsr_reg: Reg) {
        self.jit_buf
            .emit_opcodes
            .push(Mrs::cpsr(host_cpsr_reg, Cond::AL));
        self.jit_buf
            .emit_opcodes
            .extend(get_regs!(self.hle, CPU).emit_get_reg(guest_cpsr_reg, Reg::CPSR));

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
            .extend(get_regs!(self.hle, CPU).emit_set_reg(
                Reg::CPSR,
                guest_cpsr_reg,
                host_cpsr_reg,
            ));
    }
}

impl RegReserve {
    pub fn create_push_pop_handler(
        &self,
        num: u8,
        begin: &InstInfo,
        insts: &[InstInfo],
    ) -> RegPushPopHandler {
        let mut handler = RegPushPopHandler::from(self.get_writable_gp_regs(num, begin, insts));
        handler.use_gp();
        handler
    }

    pub fn get_writable_gp_regs(self, num: u8, begin: &InstInfo, insts: &[InstInfo]) -> RegReserve {
        let mut reserve = self;
        let mut writable_regs = RegReserve::new();

        if begin.op.is_branch()
            || begin.op.is_branch_thumb()
            || begin.out_regs.is_reserved(Reg::PC)
            || begin.op.requires_breakout()
            || begin.cond != Cond::AL
        {
            return writable_regs;
        }

        for inst in insts {
            if inst.op.is_branch()
                || inst.op.is_branch_thumb()
                || inst.out_regs.is_reserved(Reg::PC)
                || inst.op.requires_breakout()
                || inst.cond != Cond::AL
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

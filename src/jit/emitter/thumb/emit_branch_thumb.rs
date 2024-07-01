use crate::core::emu::get_regs;
use crate::core::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::arm::transfer_assembler::{LdrStrImm, LdrStrImmSBHD};
use crate::jit::emitter::emit_branch::{JitBranchInfo, LOCAL_BRANCH_INDICATOR};
use crate::jit::inst_branch_handler::inst_branch_label;
use crate::jit::jit_asm::{JitAsm, JitRuntimeData};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, Op};
use crate::DEBUG_LOG_BRANCH_OUT;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_b_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.insts[buf_index];

        let imm = *inst_info.operands()[0].as_imm().unwrap() as i32;
        let new_pc = (pc as i32 + 4 + imm) as u32;

        let cond = match inst_info.op {
            Op::BT => Cond::AL,
            Op::BeqT => Cond::EQ,
            Op::BneT => Cond::NE,
            Op::BcsT => Cond::HS,
            Op::BccT => Cond::LO,
            Op::BmiT => Cond::MI,
            Op::BplT => Cond::PL,
            Op::BvsT => Cond::VS,
            Op::BvcT => Cond::VC,
            Op::BhiT => Cond::HI,
            Op::BlsT => Cond::LS,
            Op::BgeT => Cond::GE,
            Op::BltT => Cond::LT,
            Op::BgtT => Cond::GT,
            Op::BleT => Cond::LE,
            _ => unreachable!(),
        };

        let mut opcodes = Vec::<u32>::new();

        let jit_asm_addr = self as *mut _ as _;
        let total_cycles = self.jit_buf.insts_cycle_counts[buf_index];

        let branch_info = Self::analyze_branch_label::<true>(&self.jit_buf.insts, buf_index, cond, pc, new_pc);
        if let Some(JitBranchInfo::Local(target_index)) = branch_info {
            let pre_cycle_count_sum = self.jit_buf.insts_cycle_counts[target_index] - self.jit_buf.insts[target_index].cycle as u16;
            let call_opcodes = self.emit_call_host_func(
                |_, opcodes| {
                    if DEBUG_LOG_BRANCH_OUT {
                        opcodes.extend(AluImm::mov32(Reg::R3, pc));
                    }
                },
                &[Some(jit_asm_addr), Some(total_cycles as u32 | ((pre_cycle_count_sum as u32) << 16)), Some(new_pc | 1), None],
                inst_branch_label::<CPU, true> as _,
            );
            opcodes.extend(call_opcodes);

            self.jit_buf.local_branches.push((pc, new_pc));
            opcodes.push(LOCAL_BRANCH_INDICATOR);
        } else {
            opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::R9));
            opcodes.push(AluImm::mov16_al(Reg::R11, self.jit_buf.insts_cycle_counts[buf_index]));

            opcodes.extend(AluImm::mov32(Reg::R10, new_pc | 1));

            if DEBUG_LOG_BRANCH_OUT {
                opcodes.extend(AluImm::mov32(Reg::R8, pc));
                opcodes.push(LdrStrImm::str_al(Reg::R8, Reg::R9));
            }
            opcodes.push(LdrStrImmSBHD::strh_al(Reg::R11, Reg::R9, JitRuntimeData::get_total_cycles_offset()));

            if let Some(JitBranchInfo::Idle) = branch_info {
                opcodes.push(AluImm::mov_al(Reg::R11, 1));
                opcodes.push(LdrStrImm::strb_offset_al(Reg::R11, Reg::R9, JitRuntimeData::get_idle_loop_offset() as u16));
            }

            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, Reg::R10, Reg::R11));

            Self::emit_host_bx(self.breakout_thumb_addr, &mut opcodes);
        }

        if cond != Cond::AL {
            self.jit_buf.emit_opcodes.push(B::b(opcodes.len() as i32 - 1, !cond));
        }

        self.jit_buf.emit_opcodes.extend(opcodes);
    }

    pub fn emit_bl_setup_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.insts[buf_index];

        let op0 = *inst_info.operands()[0].as_imm().unwrap() as i32;
        let lr = (pc as i32 + 4 + op0) as u32;

        self.jit_buf.emit_opcodes.extend(AluImm::mov32(Reg::R8, lr));
        self.jit_buf.emit_opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::LR, Reg::R8, Reg::R9));
    }

    pub fn emit_bl_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.insts[buf_index];

        let op0 = inst_info.operands()[0].as_imm().unwrap();
        let lr = (pc + 2) | 1;

        let opcodes = &mut self.jit_buf.emit_opcodes;

        opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::R11));
        opcodes.push(AluImm::mov16_al(Reg::R9, self.jit_buf.insts_cycle_counts[buf_index]));

        let thread_regs = get_regs!(self.emu, CPU);
        opcodes.extend(thread_regs.emit_get_reg(Reg::R8, Reg::LR));

        if DEBUG_LOG_BRANCH_OUT {
            opcodes.extend(AluImm::mov32(Reg::R10, pc));
            opcodes.push(LdrStrImm::str_al(Reg::R10, Reg::R11));
        }
        opcodes.push(LdrStrImmSBHD::strh_al(Reg::R9, Reg::R11, JitRuntimeData::get_total_cycles_offset()));

        if inst_info.op == Op::BlxOffT {
            opcodes.extend(AluImm::mov32(Reg::R9, *op0));
        } else {
            opcodes.extend(AluImm::mov32(Reg::R9, *op0 | 1));
        }

        opcodes.extend(AluImm::mov32(Reg::R10, lr));

        opcodes.push(AluShiftImm::add_al(Reg::R8, Reg::R8, Reg::R9));

        if inst_info.op == Op::BlxOffT {
            opcodes.push(AluImm::bic_al(Reg::R8, Reg::R8, 1));
        }

        opcodes.extend(thread_regs.emit_set_reg(Reg::LR, Reg::R10, Reg::R11));

        opcodes.extend(thread_regs.emit_set_reg(Reg::PC, Reg::R8, Reg::R9));

        Self::emit_host_bx(self.breakout_thumb_addr, opcodes);
    }

    pub fn emit_bx_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.insts[buf_index];

        let op0 = inst_info.operands()[0].as_reg_no_shift().unwrap();

        let mut reg_reserve = !(RegReserve::gp_thumb() + *op0).get_gp_regs();
        let pc_tmp_reg = reg_reserve.pop().unwrap();
        let tmp_reg = reg_reserve.pop().unwrap();
        let tmp_reg2 = reg_reserve.pop().unwrap();

        let opcodes = &mut self.jit_buf.emit_opcodes;

        opcodes.extend(AluImm::mov32(pc_tmp_reg, pc));
        opcodes.extend(self.runtime_data.emit_get_branch_out_addr(tmp_reg));
        opcodes.push(AluImm::mov16_al(tmp_reg2, self.jit_buf.insts_cycle_counts[buf_index]));

        if DEBUG_LOG_BRANCH_OUT {
            opcodes.push(LdrStrImm::str_al(pc_tmp_reg, tmp_reg));
        }
        opcodes.push(LdrStrImmSBHD::strh_al(tmp_reg2, tmp_reg, JitRuntimeData::get_total_cycles_offset()));

        if op0.is_emulated() {
            let thread_regs = get_regs!(self.emu, CPU);
            if *op0 == Reg::PC {
                opcodes.push(AluImm::add_al(tmp_reg2, pc_tmp_reg, 4));
            } else {
                opcodes.extend(thread_regs.emit_get_reg(tmp_reg2, *op0));
            }
            opcodes.extend(thread_regs.emit_set_reg(Reg::PC, tmp_reg2, tmp_reg));
        } else if op0.is_high_gp_reg() {
            let thread_regs = get_regs!(self.emu, CPU);
            opcodes.extend(thread_regs.emit_get_reg(tmp_reg, *op0));
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, tmp_reg, tmp_reg2));
        } else {
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, *op0, tmp_reg2));
        }

        if inst_info.op == Op::BlxRegT {
            opcodes.push(AluImm::add_al(tmp_reg2, pc_tmp_reg, 3));
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::LR, tmp_reg2, tmp_reg));
        }

        Self::emit_host_bx(self.breakout_thumb_addr, opcodes);
    }
}

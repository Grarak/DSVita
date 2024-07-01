use crate::core::emu::get_regs;
use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::{LdrStrImm, LdrStrImmSBHD};
use crate::jit::inst_branch_handler::inst_branch_label;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm::{JitAsm, JitRuntimeData};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::{Cond, Op};
use crate::DEBUG_LOG_BRANCH_OUT;

pub const LOCAL_BRANCH_INDICATOR: u32 = 0xFFEEDDCC;

pub enum JitBranchInfo {
    Idle,
    Local(usize),
}

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    // Taken from https://github.com/melonDS-emu/melonDS/blob/24c402af51fe9c0537582173fc48d1ad3daff459/src/ARMJIT.cpp#L352
    pub fn is_idle_loop(insts: &[InstInfo]) -> bool {
        let mut regs_written_to = RegReserve::new();
        let mut regs_disallowed_to_write = RegReserve::new();
        for (i, inst) in insts.iter().enumerate() {
            if (inst.is_branch() && i < insts.len() - 1)
                || matches!(inst.op, Op::Swi | Op::SwiT | Op::Mcr | Op::Mrc | Op::MrsRc | Op::MrsRs | Op::MsrIc | Op::MsrIs | Op::MsrRc | Op::MsrRs)
                || inst.op.mem_is_write()
            {
                return false;
            }

            let src_regs = inst.src_regs & !reg_reserve!(Reg::PC);
            let out_regs = inst.out_regs & !reg_reserve!(Reg::PC);
            regs_disallowed_to_write |= src_regs & !regs_written_to;

            if (out_regs & regs_disallowed_to_write).len() != 0 {
                return false;
            }
            regs_written_to |= out_regs;
        }
        true
    }

    pub fn analyze_branch_label<const THUMB: bool>(insts: &[InstInfo], branch_index: usize, cond: Cond, pc: u32, new_pc: u32) -> Option<JitBranchInfo> {
        if (THUMB || insts[branch_index].op != Op::Bl) && (cond as u8) < (Cond::AL as u8) && new_pc < pc {
            let diff = (pc - new_pc) >> if THUMB { 1 } else { 2 };
            if diff as usize <= branch_index {
                let jump_to_index = branch_index - diff as usize;
                if Self::is_idle_loop(&insts[jump_to_index..branch_index + 1]) {
                    return Some(JitBranchInfo::Idle);
                }
            }
        }

        if new_pc > pc {
            let diff = (new_pc - pc) >> if THUMB { 1 } else { 2 };
            if (branch_index + diff as usize) < insts.len() {
                Some(JitBranchInfo::Local(branch_index + diff as usize))
            } else {
                None
            }
        } else {
            let diff = (pc - new_pc) >> if THUMB { 1 } else { 2 };
            if branch_index >= diff as usize {
                Some(JitBranchInfo::Local(branch_index - diff as usize))
            } else {
                None
            }
        }
    }

    pub fn emit_b(&mut self, buf_index: usize, pc: u32) {
        let (op, cond, imm) = {
            let inst_info = &self.jit_buf.insts[buf_index];
            (inst_info.op, inst_info.cond, inst_info.operands()[0].as_imm().unwrap())
        };

        let new_pc = (pc as i32 + 8 + *imm as i32) as u32;

        let jit_asm_addr = self as *mut _ as _;
        let total_cycles = self.jit_buf.insts_cycle_counts[buf_index];

        let branch_info = Self::analyze_branch_label::<false>(&self.jit_buf.insts, buf_index, cond, pc, new_pc);
        if let Some(JitBranchInfo::Local(target_index)) = branch_info {
            let pre_cycle_count_sum = self.jit_buf.insts_cycle_counts[target_index] - self.jit_buf.insts[target_index].cycle as u16;
            let call_opcodes = self.emit_call_host_func(
                |_, opcodes| {
                    if op == Op::Bl {
                        opcodes.extend(AluImm::mov32(Reg::R3, pc + 4));
                        opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::LR, Reg::R3, Reg::R5));
                        if DEBUG_LOG_BRANCH_OUT {
                            opcodes.push(AluImm::sub_al(Reg::R3, Reg::R3, 4));
                        }
                    } else {
                        if DEBUG_LOG_BRANCH_OUT {
                            opcodes.extend(AluImm::mov32(Reg::R3, pc));
                        }
                    }
                },
                &[Some(jit_asm_addr), Some(total_cycles as u32 | ((pre_cycle_count_sum as u32) << 16)), Some(new_pc), None],
                inst_branch_label::<CPU, false> as _,
            );
            self.jit_buf.emit_opcodes.extend(call_opcodes);

            self.jit_buf.local_branches.push((pc, new_pc));
            self.jit_buf.emit_opcodes.push(LOCAL_BRANCH_INDICATOR);
        } else {
            let opcodes = &mut self.jit_buf.emit_opcodes;

            opcodes.extend(&get_regs!(self.emu, CPU).save_regs_opcodes);

            opcodes.extend(AluImm::mov32(Reg::R0, new_pc));
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, Reg::R0, Reg::R3));

            opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::R2));
            opcodes.push(AluImm::mov16_al(Reg::R4, total_cycles));

            if op == Op::Bl {
                opcodes.extend(AluImm::mov32(Reg::R0, pc + 4));
                opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::LR, Reg::R0, Reg::R5));
            }

            if DEBUG_LOG_BRANCH_OUT {
                opcodes.extend(AluImm::mov32(Reg::R1, pc));
                opcodes.push(LdrStrImm::str_al(Reg::R1, Reg::R2));
            }
            opcodes.push(LdrStrImmSBHD::strh_al(Reg::R4, Reg::R2, JitRuntimeData::get_total_cycles_offset()));

            if let Some(JitBranchInfo::Idle) = branch_info {
                opcodes.push(AluImm::mov_al(Reg::R4, 1));
                opcodes.push(LdrStrImm::strb_offset_al(Reg::R4, Reg::R2, JitRuntimeData::get_idle_loop_offset() as u16));
            }

            Self::emit_host_bx(self.breakout_skip_save_regs_addr, opcodes);
        }
    }

    pub fn emit_bx(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.insts[buf_index];

        let opcodes = &mut self.jit_buf.emit_opcodes;

        opcodes.extend(&get_regs!(self.emu, CPU).save_regs_opcodes);

        let reg = inst_info.operands()[0].as_reg_no_shift().unwrap();
        if *reg == Reg::LR {
            opcodes.extend(get_regs!(self.emu, CPU).emit_get_reg(Reg::R0, Reg::LR));
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, Reg::R0, Reg::LR));
        } else if *reg == Reg::PC {
            opcodes.extend(AluImm::mov32(Reg::R0, pc + 8));
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, Reg::R0, Reg::LR));
        } else {
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, *reg, Reg::LR));
        }

        opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::R2));
        opcodes.push(AluImm::mov16_al(Reg::R5, self.jit_buf.insts_cycle_counts[buf_index]));

        if inst_info.op == Op::BlxReg {
            opcodes.extend(AluImm::mov32(Reg::R3, pc + 4));
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::LR, Reg::R3, Reg::R4));
        }

        if DEBUG_LOG_BRANCH_OUT {
            opcodes.extend(AluImm::mov32(Reg::R1, pc));
            opcodes.push(LdrStrImm::str_al(Reg::R1, Reg::R2));
        }
        opcodes.push(LdrStrImmSBHD::strh_al(Reg::R5, Reg::R2, JitRuntimeData::get_total_cycles_offset()));

        Self::emit_host_bx(self.breakout_skip_save_regs_addr, opcodes);
    }

    pub fn emit_blx_label(&mut self, buf_index: usize, pc: u32) {
        if CPU != ARM9 {
            return;
        }

        let imm = {
            let inst_info = &self.jit_buf.insts[buf_index];
            inst_info.operands()[0].as_imm().unwrap()
        };

        let new_pc = (pc as i32 + 8 + *imm as i32) as u32;

        let opcodes = &mut self.jit_buf.emit_opcodes;

        opcodes.extend(&get_regs!(self.emu, CPU).save_regs_opcodes);

        opcodes.extend(AluImm::mov32(Reg::R0, new_pc | 1));

        opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, Reg::R0, Reg::R3));

        opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::R2));
        opcodes.push(AluImm::mov16_al(Reg::R3, self.jit_buf.insts_cycle_counts[buf_index]));

        opcodes.extend(AluImm::mov32(Reg::R0, pc + 4));
        opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::LR, Reg::R0, Reg::R5));

        if DEBUG_LOG_BRANCH_OUT {
            opcodes.extend(AluImm::mov32(Reg::R1, pc));
            opcodes.push(LdrStrImm::str_al(Reg::R1, Reg::R2));
        }
        opcodes.push(LdrStrImmSBHD::strh_al(Reg::R3, Reg::R2, JitRuntimeData::get_total_cycles_offset()));

        Self::emit_host_bx(self.breakout_skip_save_regs_addr, opcodes);
    }
}

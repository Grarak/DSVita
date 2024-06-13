use crate::emu::emu::get_regs;
use crate::emu::CpuType;
use crate::emu::CpuType::ARM9;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::{LdrStrImm, LdrStrImmSBHD};
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm::{JitAsm, JitRuntimeData};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::{Cond, Op};
use crate::DEBUG_LOG_BRANCH_OUT;

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

    pub fn emit_b(&mut self, buf_index: usize, pc: u32) {
        let (op, cond, imm) = {
            let inst_info = &self.jit_buf.instructions[buf_index];
            (inst_info.op, inst_info.cond, inst_info.operands()[0].as_imm().unwrap())
        };

        let new_pc = (pc as i32 + 8 + *imm as i32) as u32;

        let opcodes = &mut self.jit_buf.emit_opcodes;

        opcodes.extend(&get_regs!(self.emu, CPU).save_regs_opcodes);

        opcodes.extend(AluImm::mov32(Reg::R0, new_pc));
        opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, Reg::R0, Reg::R3));

        opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::R2));
        opcodes.push(AluImm::mov16_al(Reg::R4, self.jit_buf.insts_cycle_counts[buf_index]));

        if op == Op::Bl {
            opcodes.extend(AluImm::mov32(Reg::R0, pc + 4));
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::LR, Reg::R0, Reg::R5));
        }

        if DEBUG_LOG_BRANCH_OUT {
            opcodes.extend(AluImm::mov32(Reg::R1, pc));
            opcodes.push(LdrStrImm::str_al(Reg::R1, Reg::R2));
        }
        opcodes.push(LdrStrImmSBHD::strh_al(Reg::R4, Reg::R2, JitRuntimeData::get_total_cycles_offset()));

        if op != Op::Bl && (cond as u8) < (Cond::AL as u8) && new_pc < pc {
            let diff = (pc - new_pc) >> 2;
            if diff as usize <= buf_index {
                let jump_to_index = buf_index - diff as usize;
                if Self::is_idle_loop(&self.jit_buf.instructions[jump_to_index..buf_index + 1]) {
                    opcodes.push(AluImm::mov_al(Reg::R4, 1));
                    opcodes.push(LdrStrImm::strb_offset_al(Reg::R4, Reg::R2, JitRuntimeData::get_idle_loop_offset() as u16));
                }
            }
        }

        Self::emit_host_bx(self.breakout_skip_save_regs_addr, opcodes);
    }

    pub fn emit_bx(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

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
            let inst_info = &self.jit_buf.instructions[buf_index];
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

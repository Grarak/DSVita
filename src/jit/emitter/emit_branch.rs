use crate::hle::hle::get_regs;
use crate::hle::CpuType;
use crate::hle::CpuType::ARM9;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Op;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_b(&mut self, buf_index: usize, pc: u32) {
        let (op, imm) = {
            let inst_info = &self.jit_buf.instructions[buf_index];
            (inst_info.op, inst_info.operands()[0].as_imm().unwrap())
        };

        let new_pc = (pc as i32 + 8 + *imm as i32) as u32;
        let local_branch = if new_pc > pc {
            let diff = (new_pc - pc) >> 2;
            (buf_index + diff as usize) < self.jit_buf.instructions.len()
        } else {
            let diff = (pc - new_pc) >> 2;
            buf_index >= diff as usize
        };

        let opcodes = &mut self.jit_buf.emit_opcodes;

        opcodes.extend(&get_regs!(self.hle, CPU).save_regs_opcodes);

        opcodes.extend(AluImm::mov32(Reg::R0, new_pc));
        opcodes.extend(AluImm::mov32(Reg::R1, pc));

        opcodes.extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::PC, Reg::R0, Reg::R3));

        opcodes.extend(self.branch_out_data.emit_get_guest_pc_addr(Reg::R2));

        if op == Op::Bl {
            opcodes.push(AluImm::add_al(Reg::R0, Reg::R1, 4));
            opcodes.extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::LR, Reg::R0, Reg::R5));
        }

        opcodes.push(LdrStrImm::str_al(Reg::R1, Reg::R2));
        if local_branch {
            opcodes.push(AluImm::mov_al(Reg::R3, 1));
            opcodes.push(LdrStrImm::strb_offset_al(Reg::R3, Reg::R2, 4));
        }

        Self::emit_host_bx(self.breakout_skip_save_regs_addr, opcodes);
    }

    pub fn emit_bx(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let opcodes = &mut self.jit_buf.emit_opcodes;

        opcodes.extend(&get_regs!(self.hle, CPU).save_regs_opcodes);

        let reg = inst_info.operands()[0].as_reg_no_shift().unwrap();
        if *reg == Reg::LR {
            opcodes.extend(get_regs!(self.hle, CPU).emit_get_reg(Reg::R0, Reg::LR));
            opcodes.extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::PC, Reg::R0, Reg::LR));
        } else if *reg == Reg::PC {
            opcodes.extend(AluImm::mov32(Reg::R0, pc + 8));
            opcodes.extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::PC, Reg::R0, Reg::LR));
        } else {
            opcodes.extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::PC, *reg, Reg::LR));
        }

        opcodes.extend(AluImm::mov32(Reg::R1, pc));

        opcodes.extend(self.branch_out_data.emit_get_guest_pc_addr(Reg::R2));

        if inst_info.op == Op::BlxReg {
            opcodes.push(AluImm::add_al(Reg::R3, Reg::R1, 4));
            opcodes.extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::LR, Reg::R3, Reg::R4));
        }

        opcodes.push(LdrStrImm::str_al(Reg::R1, Reg::R2));

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

        opcodes.extend(&get_regs!(self.hle, CPU).save_regs_opcodes);

        opcodes.extend(AluImm::mov32(Reg::R0, new_pc | 1));
        opcodes.extend(AluImm::mov32(Reg::R1, pc));

        opcodes.extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::PC, Reg::R0, Reg::R3));

        opcodes.extend(self.branch_out_data.emit_get_guest_pc_addr(Reg::R2));

        opcodes.push(AluImm::add_al(Reg::R0, Reg::R1, 4));
        opcodes.extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::LR, Reg::R0, Reg::R5));

        opcodes.push(LdrStrImm::str_al(Reg::R1, Reg::R2));

        Self::emit_host_bx(self.breakout_skip_save_regs_addr, opcodes);
    }
}

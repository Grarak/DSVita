use crate::hle::hle::get_regs;
use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, Op};

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_b_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let imm = *inst_info.operands()[0].as_imm().unwrap() as i32;
        let new_pc = (pc as i32 + 4 + imm) as u32;
        let local_branch = if new_pc > pc {
            let diff = (new_pc - pc) >> 1;
            (buf_index + diff as usize) < self.jit_buf.instructions.len()
        } else {
            let diff = (pc - new_pc) >> 1;
            buf_index >= diff as usize
        };

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

        opcodes.extend(AluImm::mov32(Reg::R8, pc));
        opcodes.extend(self.branch_out_data.emit_get_guest_pc_addr(Reg::R9));
        opcodes.extend(AluImm::mov32(Reg::R10, new_pc | 1));

        opcodes.push(LdrStrImm::str_al(Reg::R8, Reg::R9));
        if local_branch {
            opcodes.push(AluImm::mov_al(Reg::R11, 1));
            opcodes.push(LdrStrImm::strb_offset_al(Reg::R11, Reg::R9, 4));
        }

        opcodes.extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::PC, Reg::R10, Reg::R11));

        Self::emit_host_bx(self.breakout_thumb_addr, &mut opcodes);

        if cond != Cond::AL {
            self.jit_buf
                .emit_opcodes
                .push(B::b(opcodes.len() as i32 - 1, !cond));
        }

        self.jit_buf.emit_opcodes.extend(opcodes);
    }

    pub fn emit_bl_setup_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let op0 = *inst_info.operands()[0].as_imm().unwrap() as i32;
        let lr = (pc as i32 + 4 + op0) as u32;

        self.jit_buf.emit_opcodes.extend(AluImm::mov32(Reg::R8, lr));
        self.jit_buf
            .emit_opcodes
            .extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::LR, Reg::R8, Reg::R9));
    }

    pub fn emit_bl_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let op0 = inst_info.operands()[0].as_imm().unwrap();
        let lr = (pc + 2) | 1;

        self.jit_buf
            .emit_opcodes
            .extend(AluImm::mov32(Reg::R10, pc));
        self.jit_buf
            .emit_opcodes
            .extend(self.branch_out_data.emit_get_guest_pc_addr(Reg::R11));

        let thread_regs = get_regs!(self.hle, CPU);
        self.jit_buf
            .emit_opcodes
            .extend(thread_regs.emit_get_reg(Reg::R8, Reg::LR));

        self.jit_buf
            .emit_opcodes
            .push(LdrStrImm::str_al(Reg::R10, Reg::R11));

        if inst_info.op == Op::BlxOffT {
            self.jit_buf
                .emit_opcodes
                .extend(AluImm::mov32(Reg::R9, *op0));
        } else {
            self.jit_buf
                .emit_opcodes
                .extend(AluImm::mov32(Reg::R9, *op0 | 1));
        }

        self.jit_buf
            .emit_opcodes
            .extend(AluImm::mov32(Reg::R10, lr));

        self.jit_buf
            .emit_opcodes
            .push(AluShiftImm::add_al(Reg::R8, Reg::R8, Reg::R9));

        if inst_info.op == Op::BlxOffT {
            self.jit_buf
                .emit_opcodes
                .push(AluImm::bic_al(Reg::R8, Reg::R8, 1));
        }

        self.jit_buf
            .emit_opcodes
            .extend(thread_regs.emit_set_reg(Reg::LR, Reg::R10, Reg::R11));

        self.jit_buf
            .emit_opcodes
            .extend(thread_regs.emit_set_reg(Reg::PC, Reg::R8, Reg::R9));

        Self::emit_host_bx(self.breakout_thumb_addr, &mut self.jit_buf.emit_opcodes);
    }

    pub fn emit_bx_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let op0 = inst_info.operands()[0].as_reg_no_shift().unwrap();

        let mut reg_reserve = !(RegReserve::gp_thumb() + *op0).get_gp_regs();
        let pc_tmp_reg = reg_reserve.pop().unwrap();
        let tmp_reg = reg_reserve.pop().unwrap();
        let tmp_reg2 = reg_reserve.pop().unwrap();

        self.jit_buf
            .emit_opcodes
            .extend(AluImm::mov32(pc_tmp_reg, pc));
        self.jit_buf
            .emit_opcodes
            .extend(self.branch_out_data.emit_get_guest_pc_addr(tmp_reg));
        self.jit_buf
            .emit_opcodes
            .push(LdrStrImm::str_al(pc_tmp_reg, tmp_reg));

        if op0.is_emulated() {
            let thread_regs = get_regs!(self.hle, CPU);
            if *op0 == Reg::PC {
                self.jit_buf
                    .emit_opcodes
                    .push(AluImm::add_al(tmp_reg2, pc_tmp_reg, 4));
            } else {
                self.jit_buf
                    .emit_opcodes
                    .extend(thread_regs.emit_get_reg(tmp_reg2, *op0));
            }
            self.jit_buf
                .emit_opcodes
                .extend(thread_regs.emit_set_reg(Reg::PC, tmp_reg2, tmp_reg));
        } else if op0.is_high_gp_reg() {
            let thread_regs = get_regs!(self.hle, CPU);
            self.jit_buf
                .emit_opcodes
                .extend(thread_regs.emit_get_reg(tmp_reg, *op0));
            self.jit_buf
                .emit_opcodes
                .extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::PC, tmp_reg, tmp_reg2));
        } else {
            self.jit_buf
                .emit_opcodes
                .extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::PC, *op0, tmp_reg2));
        }

        if inst_info.op == Op::BlxRegT {
            self.jit_buf
                .emit_opcodes
                .push(AluImm::add_al(tmp_reg2, pc_tmp_reg, 3));
            self.jit_buf
                .emit_opcodes
                .extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::LR, tmp_reg2, tmp_reg));
        }

        Self::emit_host_bx(self.breakout_thumb_addr, &mut self.jit_buf.emit_opcodes);
    }
}

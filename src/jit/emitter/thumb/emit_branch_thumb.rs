use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::jit_asm::{align_guest_pc, JitAsm};
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg};
use crate::jit::Cond;
use vixl::{Label, MasmLdr2, MasmMov2};

impl JitAsm<'_> {
    pub fn emit_bl_thumb(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let previous_inst_info = &self.jit_buf.insts[inst_index - 1];
        let relative_pc = if previous_inst_info.op != Op::BlSetupT {
            0
        } else {
            previous_inst_info.operands()[0].as_imm().unwrap() as i32
        } + 4;

        let mut target_pc = (block_asm.current_pc as i32 - 2 + relative_pc) as u32;

        let inst = &self.jit_buf.insts[inst_index];
        let op0 = inst.operands()[0].as_imm().unwrap();

        target_pc += op0;

        if inst.op == Op::BlxOffT {
            target_pc &= !1;
        } else {
            target_pc |= 1;
        }
        let is_thumb = target_pc & 1 == 1;
        target_pc = align_guest_pc(target_pc) | is_thumb as u32;

        let pc_reg = block_asm.get_guest_map(Reg::PC);
        block_asm.ldr2(pc_reg, target_pc);

        let lr_reg = block_asm.get_guest_map(Reg::LR);
        let pc = block_asm.current_pc;
        block_asm.ldr2(lr_reg, pc + 3);

        block_asm.save_dirty_guest_regs_additional(true, inst.cond == Cond::AL, reg_reserve!(Reg::LR, Reg::PC));

        self.emit_branch_external_label(inst_index, basic_block_index, target_pc, true, block_asm);
    }

    pub fn emit_b_thumb(&mut self, inst_index: usize, basic_block_index: usize, skip_label: &mut Label, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let relative_pc = inst.operands()[0].as_imm().unwrap() as i32 + 4;
        let target_pc = (block_asm.current_pc as i32 + relative_pc) as u32;

        self.emit_branch_label(inst_index, basic_block_index, target_pc | 1, skip_label, block_asm);
    }

    pub fn emit_blx_thumb(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let op0_mapped = block_asm.get_guest_map(op0);

        let pc_reg = block_asm.get_guest_map(Reg::PC);
        block_asm.mov2(pc_reg, &op0_mapped.into());

        let lr_reg = block_asm.get_guest_map(Reg::LR);
        let pc = block_asm.current_pc;
        block_asm.ldr2(lr_reg, pc + 3);

        block_asm.save_dirty_guest_regs_additional(true, inst.cond == Cond::AL, reg_reserve!(Reg::LR, Reg::PC));

        self.emit_branch_reg(inst_index, basic_block_index, pc_reg, true, block_asm);
    }
}

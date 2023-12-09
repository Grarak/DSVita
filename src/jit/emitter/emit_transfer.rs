use crate::hle::memory::indirect_memory::indirect_mem_handler::{
    indirect_mem_read, indirect_mem_write,
};
use crate::hle::memory::indirect_memory::indirect_mem_multiple_handler::{
    indirect_mem_read_multiple, indirect_mem_write_multiple,
};
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::emitter::emit::RegPushPopHandler;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Cond;
use bilge::prelude::u4;

impl JitAsm {
    fn emit_indirect(&mut self, func_addr: *const (), _: usize, pc: u32) {
        let indirect_memory_handler_addr = self.indirect_mem_handler.as_ptr() as u32;

        self.emit_call_host_func(
            |_| {},
            &[Some(indirect_memory_handler_addr), Some(pc)],
            func_addr,
        );
    }

    fn emit_memory_offset(
        jit_buf: &mut Vec<u32>,
        vm_mem_offset: u32,
        buf_index: usize,
        opcode_buf: &[InstInfo],
        inst_info: &InstInfo,
    ) {
        if inst_info.cond != Cond::AL {
            todo!()
        }

        let (reg_op1, _) = inst_info.operands()[1].as_reg().unwrap();

        let mut tmp_regs = RegPushPopHandler::from(
            inst_info
                .src_regs
                .get_writable_gp_regs(1, &opcode_buf[buf_index + 1..]),
        );
        tmp_regs.set_regs_to_skip(inst_info.src_regs);
        tmp_regs.use_gp();

        let tmp_reg = tmp_regs.pop().unwrap();

        if let Some(opcode) = tmp_regs.emit_push_stack(Reg::LR) {
            jit_buf.push(opcode);
        }

        jit_buf.extend(&AluImm::mov32(tmp_reg, vm_mem_offset));
        jit_buf.push(AluShiftImm::add_al(tmp_reg, tmp_reg, *reg_op1));

        let mut modified_op = LdrStrImm::from(inst_info.opcode);
        modified_op.set_rn(u4::new(tmp_reg as u8));
        jit_buf.push(u32::from(modified_op));

        if let Some(opcode) = tmp_regs.emit_pop_stack(Reg::LR) {
            jit_buf.push(opcode);
        }
    }

    pub fn emit_str(&mut self, buf_index: usize, pc: u32) {
        self.emit_indirect(indirect_mem_write as _, buf_index, pc);
    }

    pub fn emit_ldr_arm7(&mut self, buf_index: usize, pc: u32) {
        self.emit_indirect(indirect_mem_read as _, buf_index, pc);
    }

    pub fn emit_ldr_arm9(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let used_regs = inst_info.src_regs + inst_info.out_regs;
        let emulated_regs_count = used_regs.emulated_regs_count();
        if emulated_regs_count > 0 {
            self.handle_emulated_regs(buf_index, pc, |jit_asm, inst_info, reg_reserve| {
                let mut insts = vec![0u32; 3];

                let tmp_reg = reg_reserve.pop().unwrap();
                insts[..2].copy_from_slice(&AluImm::mov32(tmp_reg, jit_asm.vm_mem_offset));

                let (reg1, _) = inst_info.operands()[1].as_reg().unwrap();
                insts[2] = AluShiftImm::add_al(*reg1, tmp_reg, *reg1);
                insts
            });
        } else {
            JitAsm::emit_memory_offset(
                &mut self.jit_buf.emit_opcodes,
                self.vm_mem_offset,
                buf_index,
                &self.jit_buf.instructions,
                inst_info,
            );
        }
    }

    pub fn emit_stm(&mut self, buf_index: usize, pc: u32) {
        self.emit_indirect(indirect_mem_write_multiple as _, buf_index, pc);
    }

    pub fn emit_ldm_arm7(&mut self, buf_index: usize, pc: u32) {
        self.emit_indirect(indirect_mem_read_multiple as _, buf_index, pc);
    }

    pub fn emit_ldm_arm9(&mut self, buf_index: usize, pc: u32) {
        todo!()
    }
}

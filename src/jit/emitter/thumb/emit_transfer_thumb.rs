use crate::jit::inst_mem_handler::{
    inst_mem_handler_multiple_read_thumb, inst_mem_handler_multiple_write_thumb,
    inst_mem_handler_read_thumb, inst_mem_handler_write_thumb,
};
use crate::jit::jit_asm::JitAsm;

impl JitAsm {
    pub fn emit_ldr_thumb(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            inst_mem_handler_read_thumb as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_str_thumb(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            inst_mem_handler_write_thumb as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_ldm_thumb(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            inst_mem_handler_multiple_read_thumb as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_stm_thumb(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            inst_mem_handler_multiple_write_thumb as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }
}

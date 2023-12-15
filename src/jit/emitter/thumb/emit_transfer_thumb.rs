use crate::hle::memory::indirect_memory::indirect_mem_handler::{
    indirect_mem_read_thumb, indirect_mem_write_thumb,
};
use crate::hle::memory::indirect_memory::indirect_mem_multiple_handler::{
    indirect_mem_read_multiple_thumb, indirect_mem_write_multiple_thumb,
};
use crate::jit::jit_asm::JitAsm;

impl JitAsm {
    pub fn emit_ldr_thumb(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            indirect_mem_read_thumb as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_str_thumb(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            indirect_mem_write_thumb as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_ldm_thumb(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            indirect_mem_read_multiple_thumb as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_stm_thumb(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            indirect_mem_write_multiple_thumb as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }
}

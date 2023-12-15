use crate::hle::memory::indirect_memory::indirect_mem_handler::{
    indirect_mem_read, indirect_mem_write,
};
use crate::hle::memory::indirect_memory::indirect_mem_multiple_handler::{
    indirect_mem_read_multiple, indirect_mem_write_multiple,
};
use crate::jit::jit_asm::JitAsm;

impl JitAsm {
    pub fn emit_transfer_indirect(&mut self, func_addr: *const (), opcode: u32, pc: u32) {
        let indirect_memory_handler_addr = self.indirect_mem_handler.as_ptr() as u32;

        self.emit_call_host_func(
            |_| {},
            &[Some(indirect_memory_handler_addr), Some(opcode), Some(pc)],
            func_addr,
        );
    }

    pub fn emit_str(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            indirect_mem_write as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_ldr(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            indirect_mem_read as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_stm(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            indirect_mem_write_multiple as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_ldm(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            indirect_mem_read_multiple as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }
}

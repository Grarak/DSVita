use crate::hle::memory::handler::mem_handler::{
    mem_handler_read, mem_handler_write,
};
use crate::hle::memory::handler::mem_multiple_handler::{
    mem_handler_multiple_read, mem_handler_multiple_write,
};
use crate::jit::jit_asm::JitAsm;

impl JitAsm {
    pub fn emit_transfer_indirect(&mut self, func_addr: *const (), opcode: u32, pc: u32) {
        let mem_handler_addr = self.mem_handler.as_ptr() as u32;

        self.emit_call_host_func(
            |_| {},
            &[Some(mem_handler_addr), Some(opcode), Some(pc)],
            func_addr,
        );
    }

    pub fn emit_str(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            mem_handler_write as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_ldr(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            mem_handler_read as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_stm(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            mem_handler_multiple_write as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_ldm(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            mem_handler_multiple_read as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }
}

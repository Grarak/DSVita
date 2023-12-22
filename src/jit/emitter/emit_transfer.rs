use crate::jit::inst_mem_handler::{
    inst_mem_handler_multiple_read, inst_mem_handler_multiple_write, inst_mem_handler_read,
    inst_mem_handler_write,
};
use crate::jit::jit_asm::JitAsm;
use std::ptr;

impl JitAsm {
    pub fn emit_transfer_indirect(&mut self, func_addr: *const (), opcode: u32, pc: u32) {
        let mem_handler_addr = ptr::addr_of_mut!(self.inst_mem_handler) as u32;

        self.emit_call_host_func(
            |_| {},
            &[Some(mem_handler_addr), Some(opcode), Some(pc)],
            func_addr,
        );
    }

    pub fn emit_str(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            inst_mem_handler_write as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_ldr(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            inst_mem_handler_read as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_stm(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            inst_mem_handler_multiple_write as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }

    pub fn emit_ldm(&mut self, buf_index: usize, pc: u32) {
        self.emit_transfer_indirect(
            inst_mem_handler_multiple_read as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
        );
    }
}

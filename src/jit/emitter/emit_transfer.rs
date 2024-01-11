use crate::hle::CpuType;
use crate::jit::inst_mem_handler::{inst_mem_handler, inst_mem_handler_multiple};
use crate::jit::jit_asm::JitAsm;
use crate::jit::{MemoryAmount, Op};
use std::ptr;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_transfer_indirect(
        &mut self,
        func_addr: *const (),
        opcode: u32,
        pc: u32,
        flags: u8,
    ) {
        let mem_handler_addr = ptr::addr_of_mut!(self.inst_mem_handler) as u32;

        self.emit_call_host_func(
            |_| {},
            &[
                Some(mem_handler_addr),
                Some(opcode),
                Some(pc),
                Some(flags as u32),
            ],
            func_addr,
        );
    }

    pub fn emit_str(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let pre = match op {
            Op::StrOfip | Op::StrbOfip | Op::StrhOfip | Op::StrPrim => true,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::StrOfip | Op::StrbOfip | Op::StrhOfip => false,
            Op::StrPrim => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((MemoryAmount::from(op) as u8) << 2);
        self.emit_transfer_indirect(
            inst_mem_handler::<CPU, false, true> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }

    pub fn emit_ldr(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let pre = match op {
            Op::LdrOfip | Op::LdrOfim | Op::LdrbOfrplr => true,
            Op::LdrPtip => false,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::LdrOfip | Op::LdrOfim | Op::LdrbOfrplr => false,
            Op::LdrPtip => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((MemoryAmount::from(op) as u8) << 2);
        self.emit_transfer_indirect(
            inst_mem_handler::<CPU, false, false> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }

    pub fn emit_stm(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let mut pre = match op {
            Op::Stmia | Op::StmiaW => false,
            Op::Stmdb | Op::StmdbW => true,
            _ => todo!("{:?}", op),
        };

        let decrement = match op {
            Op::Stmia | Op::StmiaW => false,
            Op::Stmdb | Op::StmdbW => {
                pre = !pre;
                true
            }
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::Stmia | Op::Stmdb => false,
            Op::StmiaW | Op::StmdbW => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((decrement as u8) << 2);
        self.emit_transfer_indirect(
            inst_mem_handler_multiple::<CPU, false, true> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }

    pub fn emit_ldm(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let pre = match op {
            Op::Ldmia | Op::LdmiaW => false,
            _ => todo!("{:?}", op),
        };

        let decrement = match op {
            Op::Ldmia | Op::LdmiaW => false,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::Ldmia => false,
            Op::LdmiaW => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((decrement as u8) << 2);
        self.emit_transfer_indirect(
            inst_mem_handler_multiple::<CPU, false, false> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }
}

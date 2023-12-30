use crate::hle::CpuType;
use crate::jit::inst_mem_handler::{inst_mem_handler, inst_mem_handler_multiple};
use crate::jit::jit_asm::JitAsm;
use crate::jit::{MemoryAmount, Op};

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_ldr_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let pre = match op {
            Op::LdrshRegT
            | Op::LdrbRegT
            | Op::LdrbImm5T
            | Op::LdrImm5T
            | Op::LdrhRegT
            | Op::LdrhImm5T
            | Op::LdrPcT
            | Op::LdrSpT => true,
            Op::LdrPtip => false,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::LdrshRegT
            | Op::LdrbRegT
            | Op::LdrbImm5T
            | Op::LdrImm5T
            | Op::LdrhRegT
            | Op::LdrhImm5T
            | Op::LdrPcT
            | Op::LdrSpT => false,
            Op::LdrPtip => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((MemoryAmount::from(op) as u8) << 2);
        self.emit_transfer_indirect(
            inst_mem_handler::<CPU, true, false> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }

    pub fn emit_str_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let pre = match op {
            Op::StrbImm5T
            | Op::StrhRegT
            | Op::StrhImm5T
            | Op::StrRegT
            | Op::StrImm5T
            | Op::StrSpT => true,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::StrbImm5T
            | Op::StrhRegT
            | Op::StrhImm5T
            | Op::StrRegT
            | Op::StrImm5T
            | Op::StrSpT => false,
            Op::StrPrim => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((MemoryAmount::from(op) as u8) << 2);
        self.emit_transfer_indirect(
            inst_mem_handler::<CPU, true, true> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }

    pub fn emit_ldm_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let pre = match op {
            Op::LdmiaT | Op::PopT | Op::PopPcT => false,
            _ => todo!("{:?}", op),
        };

        let decrement = match op {
            Op::LdmiaT | Op::PopT | Op::PopPcT => false,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::LdmiaT | Op::PopT | Op::PopPcT => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((decrement as u8) << 2);
        self.emit_transfer_indirect(
            inst_mem_handler_multiple::<CPU, true, false> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }

    pub fn emit_stm_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let mut pre = match op {
            Op::PushLrT => true,
            _ => todo!("{:?}", op),
        };

        let decrement = match op {
            Op::PushLrT => {
                pre = !pre;
                true
            }
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::PushLrT => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((decrement as u8) << 2);

        self.emit_transfer_indirect(
            inst_mem_handler_multiple::<CPU, true, true> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }
}

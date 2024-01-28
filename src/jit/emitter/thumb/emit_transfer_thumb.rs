use crate::hle::CpuType;
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
            | Op::LdrRegT
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
            | Op::LdrRegT
            | Op::LdrhImm5T
            | Op::LdrPcT
            | Op::LdrSpT => false,
            Op::LdrPtip => true,
            _ => todo!("{:?}", op),
        };

        self.emit_single_transfer::<true, false>(
            buf_index,
            pc,
            pre,
            write_back,
            MemoryAmount::from(op),
        );
    }

    pub fn emit_str_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let pre = match op {
            Op::StrbRegT
            | Op::StrbImm5T
            | Op::StrhRegT
            | Op::StrhImm5T
            | Op::StrRegT
            | Op::StrImm5T
            | Op::StrSpT => true,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::StrbRegT
            | Op::StrbImm5T
            | Op::StrhRegT
            | Op::StrhImm5T
            | Op::StrRegT
            | Op::StrImm5T
            | Op::StrSpT => false,
            Op::StrPrim => true,
            _ => todo!("{:?}", op),
        };

        self.emit_single_transfer::<true, true>(
            buf_index,
            pc,
            pre,
            write_back,
            MemoryAmount::from(op),
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

        let inst_info = &self.jit_buf.instructions[buf_index];
        self.emit_multiple_transfer::<true, false>(
            pc,
            inst_info.opcode,
            inst_info.op,
            *inst_info.operands()[0].as_reg_no_shift().unwrap(),
            inst_info.cond,
            pre,
            write_back,
            decrement,
        );
    }

    pub fn emit_stm_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let mut pre = match op {
            Op::StmiaT => false,
            Op::PushT | Op::PushLrT => true,
            _ => todo!("{:?}", op),
        };

        let decrement = match op {
            Op::StmiaT => false,
            Op::PushT | Op::PushLrT => {
                pre = !pre;
                true
            }
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::StmiaT | Op::PushT | Op::PushLrT => true,
            _ => todo!("{:?}", op),
        };

        let inst_info = &self.jit_buf.instructions[buf_index];
        self.emit_multiple_transfer::<true, true>(
            pc,
            inst_info.opcode,
            inst_info.op,
            *inst_info.operands()[0].as_reg_no_shift().unwrap(),
            inst_info.cond,
            pre,
            write_back,
            decrement,
        );
    }
}

use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;
use crate::jit::MemoryAmount;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_ldr_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        self.emit_single_transfer::<true, false>(
            buf_index,
            pc,
            true,
            false,
            MemoryAmount::from(op),
        );
    }

    pub fn emit_str_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        self.emit_single_transfer::<true, true>(buf_index, pc, true, false, MemoryAmount::from(op));
    }

    pub fn emit_ldm_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let mut pre = op.mem_transfer_pre();
        let decrement = op.mem_transfer_decrement();
        if decrement {
            pre = !pre;
        }
        let write_back = op.mem_transfer_write_back();

        let inst_info = &self.jit_buf.instructions[buf_index];
        self.emit_multiple_transfer::<true, false>(
            pc,
            inst_info.opcode,
            inst_info.op,
            *inst_info.operands()[0].as_reg_no_shift().unwrap(),
            pre,
            write_back,
            decrement,
        );
    }

    pub fn emit_stm_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let mut pre = op.mem_transfer_pre();
        let decrement = op.mem_transfer_decrement();
        if decrement {
            pre = !pre;
        }
        let write_back = op.mem_transfer_write_back();

        let inst_info = &self.jit_buf.instructions[buf_index];
        self.emit_multiple_transfer::<true, true>(
            pc,
            inst_info.opcode,
            inst_info.op,
            *inst_info.operands()[0].as_reg_no_shift().unwrap(),
            pre,
            write_back,
            decrement,
        );
    }
}

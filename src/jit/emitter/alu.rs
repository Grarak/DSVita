use crate::jit::jit::JitAsm;

impl JitAsm {
    pub fn emit_mov(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.opcode_buf[buf_index];

        let used_regs = inst_info.src_regs + inst_info.out_regs;
        let emulated_regs_count = used_regs.emulated_regs_count();
        if emulated_regs_count > 0 {
            self.handle_emulated_regs(buf_index, pc, |_, _, _| Vec::new());
        } else {
            self.jit_buf.push(inst_info.opcode);
        }
    }
}

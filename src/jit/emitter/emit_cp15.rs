use crate::hle::cp15_context::{cp15_read, cp15_write};
use crate::jit::assembler::arm::alu_assembler::AluReg;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::{Cond, Op};
use std::ptr;

impl JitAsm {
    pub fn emit_cp15(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        if inst_info.cond != Cond::AL {
            todo!()
        }

        let rd = inst_info.operands()[0].as_reg_no_shift().unwrap();
        let cn = (inst_info.opcode >> 16) & 0xF;
        let cm = inst_info.opcode & 0xF;
        let cp = (inst_info.opcode >> 5) & 0x7;

        let cp15_reg = (cn << 16) | (cm << 8) | cp;
        let cp15_context_addr = self.cp15_context.as_ptr() as u32;

        let (args, addr) = match inst_info.op {
            Op::Mcr => {
                let cp15_write_addr = cp15_write as *const ();
                (
                    [Some(cp15_context_addr), Some(cp15_reg), None],
                    cp15_write_addr,
                )
            }
            Op::Mrc => {
                let reg_addr =
                    ptr::addr_of_mut!(self.thread_regs.borrow_mut().gp_regs[*rd as usize]) as u32;
                let cp15_read_addr = cp15_read as *const ();
                (
                    [Some(cp15_context_addr), Some(cp15_reg), Some(reg_addr)],
                    cp15_read_addr,
                )
            }
            _ => panic!(),
        };

        let op = inst_info.op;
        let rd = *rd;
        self.emit_call_host_func(
            |asm| {
                if op == Op::Mcr && rd != Reg::R2 {
                    asm.jit_buf.emit_opcodes.push(AluReg::mov_al(Reg::R2, rd));
                }
            },
            &args,
            addr,
        );
    }
}

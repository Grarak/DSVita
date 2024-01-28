use crate::hle::cp15_context::{cp15_read, cp15_write};
use crate::hle::cpu_regs::cpu_regs_halt;
use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::{Cond, Op};
use std::ptr;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_cp15(&mut self, buf_index: usize, pc: u32) {
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

        if cp15_reg == 0x070004 || cp15_reg == 0x070802 {
            {
                let opcodes = self.emit_call_host_func(
                    |_, _| {},
                    |_, _, _| {},
                    &[Some(self.cpu_regs.as_ref() as *const _ as u32), Some(0)],
                    cpu_regs_halt::<CPU> as *const (),
                );
                self.jit_buf.emit_opcodes.extend(opcodes);
            }

            self.jit_buf.emit_opcodes.extend(AluImm::mov32(Reg::R0, pc));
            self.jit_buf.emit_opcodes.extend(AluImm::mov32(
                Reg::R1,
                ptr::addr_of_mut!(self.guest_branch_out_pc) as u32,
            ));
            self.jit_buf
                .emit_opcodes
                .push(AluImm::add_al(Reg::R2, Reg::R0, 4));
            self.jit_buf
                .emit_opcodes
                .push(LdrStrImm::str_al(Reg::R0, Reg::R1));
            self.jit_buf
                .emit_opcodes
                .extend(
                    self.thread_regs
                        .borrow()
                        .emit_set_reg(Reg::PC, Reg::R2, Reg::R3),
                );

            Self::emit_host_bx(
                self.breakout_skip_save_regs_addr,
                &mut self.jit_buf.emit_opcodes,
            );
        } else {
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
                        self.thread_regs.borrow_mut().get_reg_value_mut(*rd) as *mut _ as u32;
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

            let opcodes = self.emit_call_host_func(
                |asm, opcodes| {
                    if op == Op::Mcr && rd != Reg::R2 {
                        opcodes.push(AluShiftImm::mov_al(Reg::R2, rd));
                    }
                },
                |_, _, _| {},
                &args,
                addr,
            );
            self.jit_buf.emit_opcodes.extend(opcodes);
        }
    }
}

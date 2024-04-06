use crate::hle::hle::{get_cp15, get_cp15_mut, get_cpu_regs_mut, get_regs, get_regs_mut};
use crate::hle::CpuType;
use crate::hle::CpuType::ARM9;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::inst_cp15_handler::{cp15_read, cp15_write};
use crate::jit::inst_cpu_regs_handler::cpu_regs_halt;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Op;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_halt<const THUMB: bool>(&mut self, pc: u32) {
        let cpu_regs_addr = get_cpu_regs_mut!(self.hle, CPU) as *mut _ as _;

        self.jit_buf.emit_opcodes.extend(self.emit_call_host_func(
            |_, _| {},
            &[Some(cpu_regs_addr), Some(0)],
            cpu_regs_halt::<CPU> as *const (),
        ));

        self.jit_buf.emit_opcodes.extend(AluImm::mov32(Reg::R0, pc));
        self.jit_buf
            .emit_opcodes
            .extend(self.branch_out_data.emit_get_guest_pc_addr(Reg::R1));
        self.jit_buf
            .emit_opcodes
            .push(AluImm::add_al(Reg::R2, Reg::R0, if THUMB { 2 } else { 4 }));
        if THUMB {
            self.jit_buf
                .emit_opcodes
                .push(AluImm::orr_al(Reg::R2, Reg::R2, 1));
        }
        self.jit_buf
            .emit_opcodes
            .push(LdrStrImm::str_al(Reg::R0, Reg::R1));
        self.jit_buf
            .emit_opcodes
            .extend(get_regs!(self.hle, CPU).emit_set_reg(Reg::PC, Reg::R2, Reg::R3));

        Self::emit_host_bx(
            self.breakout_skip_save_regs_addr,
            &mut self.jit_buf.emit_opcodes,
        );
    }

    pub fn emit_cp15(&mut self, buf_index: usize, pc: u32) {
        if CPU != ARM9 {
            return;
        }

        let hle_addr = self.hle as *mut _ as _;

        let inst_info = &self.jit_buf.instructions[buf_index];

        let rd = inst_info.operands()[0].as_reg_no_shift().unwrap();
        let cn = (inst_info.opcode >> 16) & 0xF;
        let cm = inst_info.opcode & 0xF;
        let cp = (inst_info.opcode >> 5) & 0x7;

        let cp15_reg = (cn << 16) | (cm << 8) | cp;

        if cp15_reg == 0x070004 || cp15_reg == 0x070802 {
            self.emit_halt::<false>(pc);
        } else {
            let (args, addr) = match inst_info.op {
                Op::Mcr => (
                    [
                        Some(get_cp15_mut!(self.hle, CPU) as *mut _ as _),
                        Some(cp15_reg),
                        None,
                        Some(hle_addr),
                    ],
                    cp15_write as _,
                ),
                Op::Mrc => {
                    let reg_addr = get_regs_mut!(self.hle, CPU).get_reg_mut(*rd) as *mut _ as u32;
                    (
                        [
                            Some(get_cp15!(self.hle, CPU) as *const _ as _),
                            Some(cp15_reg),
                            Some(reg_addr),
                            None,
                        ],
                        cp15_read as _,
                    )
                }
                _ => {
                    unreachable!()
                }
            };

            let op = inst_info.op;
            let rd = *rd;

            self.jit_buf.emit_opcodes.extend(self.emit_call_host_func(
                |_, opcodes| {
                    if op == Op::Mcr && rd != Reg::R2 {
                        opcodes.push(AluShiftImm::mov_al(Reg::R2, rd));
                    }
                },
                &args,
                addr,
            ));
        }
    }
}

use crate::emu::emu::{get_cp15, get_cp15_mut, get_cpu_regs_mut, get_regs, get_regs_mut};
use crate::emu::CpuType;
use crate::emu::CpuType::ARM9;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::{LdrStrImm, LdrStrImmSBHD};
use crate::jit::inst_cp15_handler::{cp15_read, cp15_write};
use crate::jit::inst_cpu_regs_handler::cpu_regs_halt;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Op;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_halt<const THUMB: bool>(&mut self, buf_index: usize, pc: u32) {
        let cpu_regs_addr = get_cpu_regs_mut!(self.emu, CPU) as *mut _ as _;

        let mut opcodes = Vec::new();

        opcodes.extend(self.emit_call_host_func(
            |_, _| {},
            &[Some(cpu_regs_addr), Some(0)],
            cpu_regs_halt::<CPU> as *const (),
        ));

        opcodes.extend(AluImm::mov32(Reg::R0, pc));
        opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::R1));
        opcodes.push(AluImm::mov16_al(
            Reg::R4,
            self.jit_buf.insts_cycle_counts[buf_index],
        ));

        opcodes.push(AluImm::add_al(Reg::R2, Reg::R0, if THUMB { 2 } else { 4 }));
        if THUMB {
            opcodes.push(AluImm::orr_al(Reg::R2, Reg::R2, 1));
        }
        opcodes.push(LdrStrImm::str_al(Reg::R0, Reg::R1));
        opcodes.push(LdrStrImmSBHD::strh_al(Reg::R4, Reg::R1, 4));

        opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, Reg::R2, Reg::R3));

        Self::emit_host_bx(self.breakout_skip_save_regs_addr, &mut opcodes);

        self.jit_buf.emit_opcodes.extend(opcodes);
    }

    pub fn emit_cp15(&mut self, buf_index: usize, pc: u32) {
        if CPU != ARM9 {
            return;
        }

        let emu_addr = self.emu as *mut _ as _;

        let inst_info = &self.jit_buf.instructions[buf_index];

        let rd = inst_info.operands()[0].as_reg_no_shift().unwrap();
        let cn = (inst_info.opcode >> 16) & 0xF;
        let cm = inst_info.opcode & 0xF;
        let cp = (inst_info.opcode >> 5) & 0x7;

        let cp15_reg = (cn << 16) | (cm << 8) | cp;

        if cp15_reg == 0x070004 || cp15_reg == 0x070802 {
            self.emit_halt::<false>(buf_index, pc);
        } else {
            let (args, addr) = match inst_info.op {
                Op::Mcr => (
                    [
                        Some(get_cp15_mut!(self.emu, CPU) as *mut _ as _),
                        Some(cp15_reg),
                        None,
                        Some(emu_addr),
                    ],
                    cp15_write as _,
                ),
                Op::Mrc => {
                    let reg_addr = get_regs_mut!(self.emu, CPU).get_reg_mut(*rd) as *mut _ as u32;
                    (
                        [
                            Some(get_cp15!(self.emu, CPU) as *const _ as _),
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

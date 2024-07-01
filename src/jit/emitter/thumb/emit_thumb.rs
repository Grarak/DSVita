use crate::core::emu::get_regs;
use crate::core::CpuType;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::{LdrStrImm, LdrStrImmSBHD};
use crate::jit::jit_asm::{JitAsm, JitRuntimeData};
use crate::jit::reg::Reg;
use crate::jit::Op;
use crate::DEBUG_LOG_BRANCH_OUT;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.insts[buf_index];
        let op = inst_info.op;
        let out_regs = inst_info.out_regs;

        let emit_func = match op {
            Op::AdcDpT
            | Op::AddImm3T
            | Op::AddImm8T
            | Op::AddRegT
            | Op::AddPcT
            | Op::AddSpT
            | Op::AndDpT
            | Op::AsrImmT
            | Op::AsrDpT
            | Op::BicDpT
            | Op::CmpDpT
            | Op::CmnDpT
            | Op::CmpImm8T
            | Op::EorDpT
            | Op::LslImmT
            | Op::LslDpT
            | Op::LsrDpT
            | Op::LsrImmT
            | Op::MovImm8T
            | Op::MulDpT
            | Op::MvnDpT
            | Op::NegDpT
            | Op::RorDpT
            | Op::SbcDpT
            | Op::SubImm3T
            | Op::SubImm8T
            | Op::SubRegT
            | Op::TstDpT
            | Op::OrrDpT => JitAsm::emit_alu_common_thumb,
            Op::AddSpImmT => JitAsm::emit_add_sp_imm_thumb,
            Op::AddHT => JitAsm::emit_add_h_thumb,
            Op::CmpHT => JitAsm::emit_cmp_h_thumb,
            Op::MovHT => JitAsm::emit_movh_thumb,

            Op::BT | Op::BeqT | Op::BneT | Op::BcsT | Op::BccT | Op::BmiT | Op::BplT | Op::BvsT | Op::BvcT | Op::BhiT | Op::BlsT | Op::BgeT | Op::BltT | Op::BgtT | Op::BleT => JitAsm::emit_b_thumb,
            Op::BlSetupT => JitAsm::emit_bl_setup_thumb,
            Op::BlOffT | Op::BlxOffT => JitAsm::emit_bl_thumb,
            Op::BxRegT | Op::BlxRegT => JitAsm::emit_bx_thumb,

            Op::SwiT => JitAsm::emit_swi::<true>,
            Op::UnkThumb => |_: &mut JitAsm<'a, CPU>, _: usize, _: u32| {},
            _ => {
                if op.is_single_mem_transfer() {
                    if inst_info.op.mem_is_write() {
                        Self::emit_str_thumb
                    } else {
                        Self::emit_ldr_thumb
                    }
                } else if op.is_multiple_mem_transfer() {
                    Self::emit_multiple_transfer::<true>
                } else {
                    todo!("{:?}", inst_info)
                }
            }
        };

        emit_func(self, buf_index, pc);

        if out_regs.is_reserved(Reg::CPSR) {
            self.handle_cpsr(Reg::R8, Reg::R9);
        }

        if out_regs.is_reserved(Reg::PC) && !op.is_multiple_mem_transfer() {
            let opcodes = &mut self.jit_buf.emit_opcodes;

            opcodes.extend(&get_regs!(self.emu, CPU).save_regs_thumb_opcodes);

            opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::LR));
            opcodes.push(AluImm::mov16_al(Reg::R3, self.jit_buf.insts_cycle_counts[buf_index]));

            let thread_regs = get_regs!(self.emu, CPU);
            opcodes.extend(thread_regs.emit_get_reg(Reg::R1, Reg::PC));
            opcodes.push(AluImm::orr_al(Reg::R1, Reg::R1, 1));
            opcodes.extend(thread_regs.emit_set_reg(Reg::PC, Reg::R1, Reg::R2));

            if DEBUG_LOG_BRANCH_OUT {
                opcodes.extend(&AluImm::mov32(Reg::R0, pc));
                opcodes.push(LdrStrImm::str_al(Reg::R0, Reg::LR));
            }
            opcodes.push(LdrStrImmSBHD::strh_al(Reg::R3, Reg::LR, JitRuntimeData::get_total_cycles_offset()));

            Self::emit_host_bx(self.breakout_skip_save_regs_thumb_addr, opcodes);
        }
    }
}

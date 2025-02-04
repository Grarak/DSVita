use crate::core::emu::get_regs_mut;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::inst_branch_handler::branch_any_reg;
use crate::jit::inst_thread_regs_handler::set_pc_thumb_mode;
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::IS_DEBUG;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    pub fn emit_thumb(&mut self, block_asm: &mut BlockAsm) {
        block_asm.guest_pc(self.jit_buf.current_pc);

        let op = self.jit_buf.current_inst().op;
        match op {
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
            | Op::OrrDpT => self.emit_alu_common_thumb(block_asm),
            Op::AddSpImmT => self.emit_add_sp_imm_thumb(block_asm),
            Op::AddHT => self.emit_add_h_thumb(block_asm),
            Op::CmpHT => self.emit_cmp_h_thumb(block_asm),
            Op::MovHT => self.emit_movh_thumb(block_asm),

            Op::BT | Op::BeqT | Op::BneT | Op::BcsT | Op::BccT | Op::BmiT | Op::BplT | Op::BvsT | Op::BvcT | Op::BhiT | Op::BlsT | Op::BgeT | Op::BltT | Op::BgtT | Op::BleT => {
                self.emit_b_thumb(block_asm)
            }
            Op::BlSetupT => {}
            Op::BlOffT | Op::BlxOffT => self.emit_bl_thumb(block_asm),
            Op::BxRegT => self.emit_bx_thumb(block_asm),
            Op::BlxRegT => self.emit_blx_thumb(block_asm),

            Op::SwiT => self.emit_swi::<true>(block_asm),
            Op::UnkThumb => unreachable!(),
            op if op.is_single_mem_transfer() => {
                if op.mem_is_write() {
                    self.emit_str_thumb(block_asm)
                } else {
                    self.emit_ldr_thumb(block_asm)
                }
            }
            op if op.is_multiple_mem_transfer() => self.emit_multiple_transfer(block_asm, true),
            _ => {
                todo!("{:?}", self.jit_buf.current_inst())
            }
        }

        if self.jit_buf.current_inst().out_regs.is_reserved(Reg::PC) {
            block_asm.save_context();

            if CPU == ARM7 || !op.is_multiple_mem_transfer() {
                block_asm.call1(set_pc_thumb_mode as *const (), get_regs_mut!(self.emu, CPU) as *mut _ as u32);
            }

            // R9 can be used as a substitution for SP for branch prediction
            if (op == Op::MovHT && self.jit_buf.current_inst().src_regs.is_reserved(Reg::LR))
                || (op.is_multiple_mem_transfer() && matches!(*self.jit_buf.current_inst().operands()[0].as_reg_no_shift().unwrap(), Reg::R9 | Reg::SP))
                || (op.is_single_mem_transfer() && (self.jit_buf.current_inst().src_regs.is_reserved(Reg::R9) || self.jit_buf.current_inst().src_regs.is_reserved(Reg::SP)))
            {
                let guest_pc_reg = block_asm.new_reg();
                block_asm.load_u32(guest_pc_reg, block_asm.tmp_regs.thread_regs_addr_reg, Reg::PC as u32 * 4);
                self.emit_branch_return_stack_common(block_asm, guest_pc_reg);
                block_asm.free_reg(guest_pc_reg);
            } else if CPU == ARM9 {
                if IS_DEBUG {
                    block_asm.call2_no_return(branch_any_reg as *const (), self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32, self.jit_buf.current_pc);
                } else {
                    block_asm.call1_no_return(branch_any_reg as *const (), self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32);
                }
            } else {
                self.emit_branch_out_metadata(block_asm);
                block_asm.epilogue();
            }
        }
    }
}

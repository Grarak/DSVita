use crate::core::CpuType;
use crate::core::CpuType::ARM7;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::inst_threag_regs_handler::set_pc_thumb_mode;
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
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
            Op::UnkThumb => {}
            op if op.is_single_mem_transfer() => {
                if op.mem_is_write() {
                    self.emit_str_thumb(block_asm)
                } else {
                    self.emit_ldr_thumb(block_asm)
                }
            }
            op if op.is_multiple_mem_transfer() => self.emit_multiple_transfer::<true>(block_asm),
            _ => {
                todo!("{:?}", self.jit_buf.current_inst())
            }
        }

        if self.jit_buf.current_inst().out_regs.is_reserved(Reg::PC) {
            block_asm.save_context();

            if CPU == ARM7 || !op.is_multiple_mem_transfer() {
                block_asm.call(set_pc_thumb_mode::<CPU> as *const ());
            }

            self.emit_branch_out_metadata(block_asm);
            block_asm.epilogue();
        }
    }
}

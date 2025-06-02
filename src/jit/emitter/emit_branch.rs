use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::vixl::vixl::{BranchHint_kFar, FlagsUpdate_DontCare, MemOperand};
use crate::jit::assembler::vixl::{Label, MasmB2, MasmB3, MasmBic5, MasmBlx1, MasmBx1, MasmCmp2, MasmLdr2, MasmLdrb2, MasmMov2, MasmMov4, MasmOrr5, MasmStrb2, MasmStrh2};
use crate::jit::inst_branch_handler::{branch_lr, branch_reg, handle_idle_loop, handle_interrupt, pre_branch};
use crate::jit::jit_asm::{JitAsm, JitRuntimeData};
use crate::jit::jit_asm_common_funs::{get_max_loop_cycle_count, JitAsmCommonFuns};
use crate::jit::reg::{reg_reserve, Reg};
use crate::jit::{inst_branch_handler, Cond};
use crate::logging::branch_println;
use crate::settings::Arm7Emu;
use crate::{BRANCH_LOG, IS_DEBUG};
use std::ptr;
use CpuType::ARM7;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    fn emit_set_cpsr_thumb_bit_imm(&mut self, thumb: bool, block_asm: &mut BlockAsm) {
        block_asm.load_guest_reg(Reg::R0, Reg::CPSR);
        if thumb {
            block_asm.orr5(FlagsUpdate_DontCare, Cond::AL, Reg::R0, Reg::R0, &(1 << 5).into());
        } else {
            block_asm.bic5(FlagsUpdate_DontCare, Cond::AL, Reg::R0, Reg::R0, &(1 << 5).into());
        }
        block_asm.store_guest_reg(Reg::R0, Reg::CPSR);
    }

    fn emit_call_jit_addr_imm(&mut self, target_pc: u32, has_return: bool, block_asm: &mut BlockAsm) {
        self.emit_set_cpsr_thumb_bit_imm(target_pc & 1 == 1, block_asm);

        let jit_entry_addr = self.emu.jit.jit_memory_map.get_jit_entry(target_pc);
        block_asm.ldr2(Reg::R0, target_pc);
        block_asm.ldr2(Reg::R1, jit_entry_addr as u32);
        block_asm.ldr2(Reg::R3, &MemOperand::reg(Reg::R1));
        if has_return {
            block_asm.blx1(Reg::R3);
        } else {
            block_asm.bx1(Reg::R3);
        }
    }

    pub fn emit_call_branch_imm(&mut self, inst_index: usize, target_pc: u32, has_return: bool, block_asm: &mut BlockAsm) {
        block_asm.ldr2(Reg::R0, self as *mut _ as u32);
        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &self.jit_buf.insts_cycle_counts[inst_index].into());
        if has_return {
            let lr_reg = block_asm.get_guest_map(Reg::LR);
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R2, &lr_reg.into());
        }
        if IS_DEBUG {
            let pc = block_asm.current_pc;
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R3, &pc.into());
        }
        block_asm.bl(if has_return { pre_branch::<CPU, true> as _ } else { pre_branch::<CPU, false> as _ });

        if BRANCH_LOG {
            let pc = block_asm.current_pc;
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &pc.into());
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &target_pc.into());
            block_asm.bl(JitAsmCommonFuns::<CPU>::debug_branch_imm as _);
        }

        block_asm.restore_guest_regs_ptr();
        if !has_return {
            block_asm.restore_stack();
        }
        self.emit_call_jit_addr_imm(target_pc, has_return, block_asm);
    }

    pub fn emit_call_branch_reg(&mut self, inst_index: usize, target_pc_reg: Reg, has_return: bool, block_asm: &mut BlockAsm) {
        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &self.jit_buf.insts_cycle_counts[inst_index].into());
        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &target_pc_reg.into());
        if has_return {
            let lr_reg = block_asm.get_guest_map(Reg::LR);
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R2, &lr_reg.into());
        }
        if IS_DEBUG {
            let pc = block_asm.current_pc;
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R3, &pc.into());
        }

        if has_return {
            block_asm.bl(branch_reg::<CPU, true> as _);
        } else {
            block_asm.restore_stack();
            block_asm.b(branch_reg::<CPU, false> as _);
        }
    }

    pub fn emit_branch_external_label(&mut self, inst_index: usize, basic_block_index: usize, target_pc: u32, has_return: bool, block_asm: &mut BlockAsm) {
        if has_return {
            self.emit_call_branch_imm(inst_index, target_pc, true, block_asm);
            block_asm.ldr2(Reg::R1, ptr::addr_of_mut!(self.runtime_data) as u32);

            if inst_index == self.jit_buf.insts.len() - 1 {
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &0.into());
                block_asm.strh2(Reg::R0, &MemOperand::reg_offset(Reg::R1, JitRuntimeData::get_pre_cycle_count_sum_offset() as i32));

                block_asm.restore_guest_regs_ptr();
                block_asm.restore_stack();
                let lr = if block_asm.thumb { block_asm.current_pc + 3 } else { block_asm.current_pc + 4 };
                self.emit_call_jit_addr_imm(lr, false, block_asm);
                return;
            }

            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &self.jit_buf.insts_cycle_counts[inst_index].into());
            block_asm.strh2(Reg::R0, &MemOperand::reg_offset(Reg::R1, JitRuntimeData::get_pre_cycle_count_sum_offset() as i32));

            let mut next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
            if self.jit_buf.insts[inst_index].cond != Cond::AL {
                next_live_regs += Reg::CPSR;
            }
            block_asm.restore_tmp_regs(next_live_regs);

            block_asm.reload_active_guest_regs_all();
        } else {
            self.emit_call_branch_imm(inst_index, target_pc, false, block_asm);
        }
    }

    pub fn emit_branch_reg(&mut self, inst_index: usize, basic_block_index: usize, target_pc_reg: Reg, has_return: bool, block_asm: &mut BlockAsm) {
        self.emit_call_branch_reg(inst_index, target_pc_reg, has_return, block_asm);

        if has_return {
            if inst_index == self.jit_buf.insts.len() - 1 {
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &0.into());
                block_asm.ldr2(Reg::R1, ptr::addr_of_mut!(self.runtime_data) as u32);
                block_asm.strh2(Reg::R0, &MemOperand::reg_offset(Reg::R1, JitRuntimeData::get_pre_cycle_count_sum_offset() as i32));

                block_asm.restore_guest_regs_ptr();
                block_asm.restore_stack();
                let lr = if block_asm.thumb { block_asm.current_pc + 3 } else { block_asm.current_pc + 4 };
                self.emit_call_jit_addr_imm(lr, false, block_asm);
                return;
            }

            let mut next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
            if self.jit_buf.insts[inst_index].cond != Cond::AL {
                next_live_regs += Reg::CPSR;
            }
            block_asm.restore_tmp_regs(next_live_regs);

            block_asm.reload_active_guest_regs_all();
        }
    }

    pub fn emit_branch_return_stack(&mut self, inst_index: usize, target_pc_reg: Reg, block_asm: &mut BlockAsm) {
        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &self.jit_buf.insts_cycle_counts[inst_index].into());
        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &target_pc_reg.into());
        if IS_DEBUG {
            let pc = block_asm.current_pc;
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R2, &pc.into());
        }

        block_asm.restore_stack();
        block_asm.b(branch_lr::<CPU> as _);
    }

    pub fn emit_branch_label(&mut self, inst_index: usize, basic_block_index: usize, target_pc: u32, pc_reg: Reg, block_asm: &mut BlockAsm) {
        let thumb = target_pc & 1 == 1;
        let aligned_target_pc = target_pc & !1;
        let pc_shift = if thumb { 1 } else { 2 };

        let metadata = self.analyzer.insts_metadata[inst_index];
        if metadata.idle_loop() {
            if BRANCH_LOG {
                let pc = block_asm.current_pc;
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &pc.into());
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &pc_reg.into());
                block_asm.bl(Self::debug_idle_loop as _);
            }

            match CPU {
                ARM9 => {
                    let jump_to_index = inst_index - ((block_asm.current_pc - aligned_target_pc) >> pc_shift) as usize;
                    let target_pre_cycle_count_sum = self.jit_buf.insts_cycle_counts[jump_to_index] - self.jit_buf.insts[jump_to_index].cycle as u16;
                    block_asm.ldr2(Reg::R0, self as *mut _ as u32);
                    block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &target_pre_cycle_count_sum.into());
                    if IS_DEBUG {
                        let pc = block_asm.current_pc;
                        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R2, &pc.into());
                    }
                    if self.emu.settings.arm7_hle() == Arm7Emu::Hle {
                        block_asm.bl(handle_idle_loop::<true> as _);
                    } else {
                        block_asm.bl(handle_idle_loop::<false> as _);
                    }
                    let basic_block_index = self.analyzer.get_basic_block_from_inst(jump_to_index);
                    block_asm.b_basic_block(basic_block_index);
                }
                ARM7 => {
                    self.emit_branch_out_metadata(inst_index, true, block_asm);
                    let mem_operand = MemOperand::reg_offset(Reg::R0, JitRuntimeData::get_data_packed_offset() as i32 + 3);
                    block_asm.ldrb2(Reg::R1, &mem_operand);
                    block_asm.orr5(FlagsUpdate_DontCare, Cond::AL, Reg::R1, Reg::R1, &0x80.into());
                    block_asm.strb2(Reg::R1, &mem_operand);
                    block_asm.exit_guest_context(&mut self.runtime_data.host_sp);
                }
            }
        } else if metadata.external_branch() {
            self.emit_branch_external_label(inst_index, basic_block_index, target_pc, false, block_asm);
        } else {
            let jump_to_index = (inst_index as isize + ((aligned_target_pc as isize - block_asm.current_pc as isize) >> pc_shift)) as usize;
            let target_pre_cycle_count_sum = self.jit_buf.insts_cycle_counts[jump_to_index] - self.jit_buf.insts[jump_to_index].cycle as u16;

            block_asm.ldr2(Reg::R0, ptr::addr_of_mut!(self.runtime_data) as u32);
            self.emit_count_cycles(self.jit_buf.insts_cycle_counts[inst_index], block_asm);

            let mut cycles_exceed_label = Label::new();
            let mut continue_label = Label::new();
            let mut exit_label = Label::new();

            block_asm.cmp2(Reg::R3, &get_max_loop_cycle_count::<CPU>().into());
            block_asm.b3(Cond::HS, &mut cycles_exceed_label, BranchHint_kFar);

            block_asm.bind(&mut continue_label);

            let current_pc_jit_entry = self.emu.jit.jit_memory_map.get_jit_entry(block_asm.current_pc);
            let target_pc_jit_entry = self.emu.jit.jit_memory_map.get_jit_entry(aligned_target_pc);

            if jump_to_index > inst_index {
                block_asm.ldr2(Reg::R0, current_pc_jit_entry as u32);
                block_asm.ldr2(Reg::R1, target_pc_jit_entry as u32);
                block_asm.ldr2(Reg::R0, &MemOperand::reg(Reg::R0));
                block_asm.ldr2(Reg::R1, &MemOperand::reg(Reg::R1));
                block_asm.cmp2(Reg::R0, &Reg::R1.into());
                block_asm.b3(Cond::NE, &mut exit_label, BranchHint_kFar);
            }

            block_asm.ldr2(Reg::R0, ptr::addr_of_mut!(self.runtime_data) as u32);
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &target_pre_cycle_count_sum.into());
            block_asm.strh2(Reg::R1, &MemOperand::reg_offset(Reg::R0, JitRuntimeData::get_pre_cycle_count_sum_offset() as i32));

            if BRANCH_LOG {
                let pc = block_asm.current_pc;
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &pc.into());
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &pc_reg.into());
                block_asm.bl(Self::debug_branch_label as _);
            }

            let basic_block_index = self.analyzer.get_basic_block_from_inst(jump_to_index);
            block_asm.b_basic_block(basic_block_index);

            block_asm.bind(&mut cycles_exceed_label);

            match CPU {
                ARM9 => {
                    block_asm.ldr2(Reg::R0, self as *mut _ as u32);
                    if IS_DEBUG {
                        let pc = block_asm.current_pc;
                        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &pc.into());
                    }
                    if self.emu.settings.arm7_hle() == Arm7Emu::Hle {
                        block_asm.bl(inst_branch_handler::run_scheduler::<true> as _);
                    } else {
                        block_asm.bl(inst_branch_handler::run_scheduler::<false> as _);
                    };

                    block_asm.restore_guest_regs_ptr();
                    block_asm.load_guest_reg(Reg::R0, Reg::PC);
                    block_asm.cmp2(Reg::R0, &pc_reg.into());

                    block_asm.b3(Cond::EQ, &mut continue_label, BranchHint_kFar);

                    block_asm.ldr2(Reg::R0, self as *mut _ as u32);
                    block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &pc_reg.into());
                    if IS_DEBUG {
                        let pc = block_asm.current_pc;
                        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R2, &pc.into());
                    }
                    block_asm.bl(handle_interrupt as _);
                    block_asm.b2(&mut continue_label, BranchHint_kFar);

                    if jump_to_index > inst_index {
                        block_asm.bind(&mut exit_label);
                        self.emit_branch_out_metadata(inst_index, false, block_asm);
                        block_asm.exit_guest_context(&mut self.runtime_data.host_sp);
                    }
                }
                ARM7 => {
                    block_asm.bind(&mut exit_label);
                    self.emit_branch_out_metadata(inst_index, false, block_asm);
                    block_asm.exit_guest_context(&mut self.runtime_data.host_sp);
                }
            }
        }
    }

    pub fn emit_blx_reg(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let op0_mapped = block_asm.get_guest_map(op0);

        let pc_reg = block_asm.get_guest_map(Reg::PC);
        block_asm.mov2(pc_reg, &op0_mapped.into());

        let lr_reg = block_asm.get_guest_map(Reg::LR);
        let pc = block_asm.current_pc;
        block_asm.ldr2(lr_reg, pc + 4);

        block_asm.save_dirty_guest_regs_additional(true, inst.cond == Cond::AL, reg_reserve!(Reg::LR, Reg::PC));

        self.emit_branch_reg(inst_index, basic_block_index, pc_reg, true, block_asm);
    }

    pub fn emit_bl(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let relative_pc = inst.operands()[0].as_imm().unwrap() as i32 + 8;
        let target_pc = (block_asm.current_pc as i32 + relative_pc) as u32;

        let pc_reg = block_asm.get_guest_map(Reg::PC);
        block_asm.ldr2(pc_reg, target_pc);

        let lr_reg = block_asm.get_guest_map(Reg::LR);
        let pc = block_asm.current_pc;
        block_asm.ldr2(lr_reg, pc + 4);
        block_asm.save_dirty_guest_regs_additional(true, inst.cond == Cond::AL, reg_reserve!(Reg::LR, Reg::PC));

        self.emit_branch_external_label(inst_index, basic_block_index, target_pc, true, block_asm);
    }

    pub fn emit_b(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let relative_pc = inst.operands()[0].as_imm().unwrap() as i32 + 8;
        let target_pc = (block_asm.current_pc as i32 + relative_pc) as u32;

        let pc_reg = block_asm.get_guest_map(Reg::PC);
        block_asm.ldr2(pc_reg, target_pc);

        block_asm.save_dirty_guest_regs_additional(true, inst.cond == Cond::AL, reg_reserve!(Reg::PC));

        self.emit_branch_label(inst_index, basic_block_index, target_pc, pc_reg, block_asm);
    }

    pub fn emit_bx(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let op0_mapped = block_asm.get_guest_map(op0);

        let pc_reg = block_asm.get_guest_map(Reg::PC);
        block_asm.mov2(pc_reg, &op0_mapped.into());
        block_asm.save_dirty_guest_regs_additional(true, inst.cond == Cond::AL, reg_reserve!(Reg::PC));

        if op0 == Reg::LR {
            self.emit_branch_return_stack(inst_index, pc_reg, block_asm);
        } else {
            self.emit_branch_reg(inst_index, basic_block_index, pc_reg, false, block_asm);
        }
    }

    pub fn emit_blx(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        if CPU != ARM9 {
            return;
        }

        let inst = &self.jit_buf.insts[inst_index];
        let relative_pc = inst.operands()[0].as_imm().unwrap() as i32 + 8;
        let target_pc = (block_asm.current_pc as i32 + relative_pc) as u32;

        let pc_reg = block_asm.get_guest_map(Reg::PC);
        block_asm.ldr2(pc_reg, target_pc | 1);

        let lr_reg = block_asm.get_guest_map(Reg::LR);
        let pc = block_asm.current_pc;
        block_asm.ldr2(lr_reg, pc + 4);
        block_asm.save_dirty_guest_regs_additional(true, inst.cond == Cond::AL, reg_reserve!(Reg::LR, Reg::PC));

        self.emit_branch_external_label(inst_index, basic_block_index, target_pc | 1, true, block_asm);
    }

    extern "C" fn debug_branch_label(current_pc: u32, target_pc: u32) {
        branch_println!("{CPU:?} branch label from {current_pc:x} to {target_pc:x}")
    }

    extern "C" fn debug_branch_reg(current_pc: u32, target_pc: u32) {
        branch_println!("{CPU:?} branch reg from {current_pc:x} to {target_pc:x}")
    }

    extern "C" fn debug_idle_loop(current_pc: u32, target_pc: u32) {
        branch_println!("{CPU:?} detected idle loop {current_pc:x} to {target_pc:x}")
    }
}

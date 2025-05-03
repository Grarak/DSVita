use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::vixl::vixl::{AddrMode_PostIndex, AddrMode_PreIndex, FlagsUpdate_DontCare, MemOperand, WriteBack};
use crate::jit::assembler::vixl::{
    MacroAssembler, MasmAdd3, MasmAdd5, MasmAnd5, MasmBic3, MasmLdm3, MasmLdmda3, MasmLdmdb3, MasmLdmib3, MasmLdr2, MasmLdrb2, MasmLdrh2, MasmLsl5, MasmMov2, MasmMov4, MasmMovs2, MasmNop, MasmRor5,
    MasmStm3, MasmStmda3, MasmStmdb3, MasmStmib3, MasmStr2, MasmStrb2, MasmStrh2, MasmSub3, MasmSub5,
};
use crate::jit::inst_mem_handler::{inst_mem_handler_multiple_slow, InstMemMultipleParams};
use crate::jit::jit_asm::JitAsm;
use crate::jit::jit_memory::{
    SLOW_MEM_MULTIPLE_LENGTH_ARM, SLOW_MEM_MULTIPLE_LENGTH_THUMB, SLOW_MEM_SINGLE_READ_LENGTH_ARM, SLOW_MEM_SINGLE_READ_LENGTH_THUMB, SLOW_MEM_SINGLE_WRITE_LENGTH_ARM,
    SLOW_MEM_SINGLE_WRITE_LENGTH_THUMB,
};
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use bilge::arbitrary_int::{u4, u6};
use std::cmp::min;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    pub fn emit_single_transfer(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        let transfer = match inst.op {
            Op::Ldr(transfer) | Op::LdrT(transfer) | Op::Str(transfer) | Op::StrT(transfer) => transfer,
            _ => unreachable!(),
        };

        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let op1 = inst.operands()[1].as_reg_no_shift().unwrap();
        let op2 = &inst.operands()[2];

        let op0_mapped = block_asm.get_guest_map(op0);
        let op1_mapped = block_asm.get_guest_map(op1);
        let op2_mapped = block_asm.get_guest_operand_map(op2);

        if transfer.add() {
            block_asm.add3(Reg::R3, op1_mapped, &op2_mapped);
        } else {
            block_asm.sub3(Reg::R3, op1_mapped, &op2_mapped);
        }

        if inst.op == Op::LdrT(transfer) && op1 == Reg::PC {
            block_asm.bic3(Reg::R3, Reg::R3, &0x3.into());
        }

        if transfer.pre() {
            block_asm.mov2(Reg::R2, &Reg::R3.into());
        } else {
            block_asm.mov2(Reg::R2, &op1_mapped.into());
        }

        let size = 1 << transfer.size();
        let is_64bit = size == 8;
        let size = if is_64bit { 4 } else { size } as u32;

        block_asm.save_dirty_guest_cpsr(inst.cond == Cond::AL);

        if inst.op.is_write_mem_transfer() {
            block_asm.mov2(Reg::R0, &op0_mapped.into());
            if is_64bit {
                let next_reg = block_asm.get_guest_map(Reg::from(op0 as u8 + 1));
                block_asm.mov2(Reg::R1, &next_reg.into());
            }
        } else {
            block_asm.movs2(Reg::R0, &(op0 as u8).into());
        }

        if transfer.write_back() && (inst.op.is_write_mem_transfer() || op0 != op1) {
            block_asm.mov2(op1_mapped, &Reg::R3.into());
            block_asm.add_dirty_guest_regs(reg_reserve!(op1));
        }
        block_asm.save_dirty_guest_regs(false, inst.cond == Cond::AL);

        let fast_mem_start = block_asm.get_cursor_offset();
        block_asm.nop0();

        block_asm.ldr2(Reg::R3, !(0xF0000000 | (size - 1)));
        block_asm.and5(FlagsUpdate_DontCare, Cond::AL, Reg::R3, Reg::R3, &Reg::R2.into());

        let func = if inst.op.is_write_mem_transfer() {
            match size {
                1 => <MacroAssembler as MasmStrb2<Reg, &MemOperand>>::strb2,
                2 => <MacroAssembler as MasmStrh2<_, _>>::strh2,
                4 | 8 => <MacroAssembler as MasmStr2<_, _>>::str2,
                _ => unreachable!(),
            }
        } else {
            match size {
                1 => <MacroAssembler as MasmLdrb2<Reg, &MemOperand>>::ldrb2,
                2 => <MacroAssembler as MasmLdrh2<_, _>>::ldrh2,
                4 | 8 => <MacroAssembler as MasmLdr2<_, _>>::ldr2,
                _ => unreachable!(),
            }
        };

        let block_offset = block_asm.guest_inst_metadata(self.jit_buf.insts_cycle_counts[inst_index], inst, RegReserve::new()) as u32;
        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R12, &block_offset.into());
        block_asm.load_mmu_offset(Reg::LR);
        let mem_operand = MemOperand::reg_offset2(Reg::R3, Reg::LR);
        func(block_asm, Reg::R0, &mem_operand);
        if !inst.op.is_write_mem_transfer() && size == 4 {
            block_asm.lsl5(FlagsUpdate_DontCare, Cond::AL, Reg::R1, Reg::R2, &3.into());
            block_asm.ror5(FlagsUpdate_DontCare, Cond::AL, Reg::R0, Reg::R0, &Reg::R1.into());
        }
        if is_64bit {
            block_asm.add5(FlagsUpdate_DontCare, Cond::AL, Reg::R2, Reg::R2, &4.into());
            block_asm.ldr2(Reg::R3, !0xF0000003);
            block_asm.and5(FlagsUpdate_DontCare, Cond::AL, Reg::R3, Reg::R3, &Reg::R2.into());

            func(block_asm, Reg::R1, &mem_operand);
            if !inst.op.is_write_mem_transfer() {
                block_asm.lsl5(FlagsUpdate_DontCare, Cond::AL, Reg::R2, Reg::R2, &3.into());
                block_asm.ror5(FlagsUpdate_DontCare, Cond::AL, Reg::R1, Reg::R1, &Reg::R2.into());
            }
        }

        block_asm.nop0();
        let fast_mem_end = block_asm.get_cursor_offset();
        let fast_mem_size = fast_mem_end - fast_mem_start;

        let (slow_mem_length, step_size) = match (inst.op.is_write_mem_transfer(), block_asm.thumb) {
            (false, false) => (SLOW_MEM_SINGLE_READ_LENGTH_ARM, 4),
            (false, true) => (SLOW_MEM_SINGLE_READ_LENGTH_THUMB, 2),
            (true, false) => (SLOW_MEM_SINGLE_WRITE_LENGTH_ARM, 4),
            (true, true) => (SLOW_MEM_SINGLE_WRITE_LENGTH_THUMB, 2),
        };
        for _ in (fast_mem_size as usize..slow_mem_length).step_by(step_size) {
            block_asm.nop0();
        }

        if !inst.op.is_write_mem_transfer() {
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, op0_mapped, &Reg::R0.into());
            if is_64bit {
                let next_reg = block_asm.get_guest_map(Reg::from(op0 as u8 + 1));
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, next_reg, &Reg::R1.into());
            }
        }

        let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
        block_asm.restore_tmp_regs(next_live_regs);
    }

    pub fn emit_multiple_transfer(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);

        let transfer = match inst.op {
            Op::Ldm(transfer) | Op::LdmT(transfer) | Op::Stm(transfer) | Op::StmT(transfer) => transfer,
            _ => unreachable!(),
        };

        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let mut op1 = inst.operands()[1].as_reg_list().unwrap();

        block_asm.alloc_guest_regs(reg_reserve!(op0), op1 & Reg::PC, next_live_regs);

        let op0_mapped = block_asm.get_guest_map(op0);
        let op1_len = op1.len();

        block_asm.save_dirty_guest_regs(true, inst.cond == Cond::AL);

        let use_fast_mem = !transfer.user() && !op1.is_empty() && (!transfer.write_back() || !op1.is_reserved(op0));
        if use_fast_mem {
            let fast_mem_start = block_asm.get_cursor_offset();
            block_asm.nop0();
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &op0_mapped.into());
            block_asm.load_mmu_offset(Reg::R1);
            block_asm.add5(FlagsUpdate_DontCare, Cond::AL, Reg::R0, Reg::R0, &Reg::R1.into());

            const USABLE_REGS: RegReserve = reg_reserve!(Reg::R2, Reg::R3, Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::LR);
            let mut remaining_op1 = op1_len;
            while remaining_op1 != 0 {
                let len = min(remaining_op1, USABLE_REGS.len());
                let guest_regs = op1.into_iter().take(len).collect::<RegReserve>();
                let usable_regs = USABLE_REGS.into_iter().take(len).collect::<RegReserve>();
                if inst.op.is_write_mem_transfer() {
                    for (guest_reg, usable_reg) in guest_regs.into_iter().zip(usable_regs) {
                        block_asm.load_guest_reg(usable_reg, guest_reg);
                    }

                    let block_offset = block_asm.guest_inst_metadata(self.jit_buf.insts_cycle_counts[inst_index], inst, op1) as u32;
                    block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R12, &block_offset.into());

                    if remaining_op1 == 1 {
                        let usable_reg = USABLE_REGS.peek().unwrap();
                        block_asm.str2(usable_reg, unsafe {
                            &MemOperand::new1(Reg::R0.into(), if transfer.add() { 4 } else { -4 }, if transfer.pre() { AddrMode_PreIndex } else { AddrMode_PostIndex })
                        });
                    } else {
                        let func = match (transfer.pre(), transfer.add()) {
                            (false, false) => <MacroAssembler as MasmStmda3<Reg, WriteBack, RegReserve>>::stmda3,
                            (false, true) => <MacroAssembler as MasmStm3<_, _, _>>::stm3,
                            (true, false) => <MacroAssembler as MasmStmdb3<_, _, _>>::stmdb3,
                            (true, true) => <MacroAssembler as MasmStmib3<_, _, _>>::stmib3,
                        };
                        func(block_asm, Reg::R0, WriteBack::yes(), usable_regs);
                    }

                    block_asm.restore_guest_regs_ptr();
                } else {
                    let block_offset = block_asm.guest_inst_metadata(self.jit_buf.insts_cycle_counts[inst_index], inst, op1) as u32;
                    block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R12, &block_offset.into());

                    if remaining_op1 == 1 {
                        let usable_reg = USABLE_REGS.peek().unwrap();
                        block_asm.ldr2(usable_reg, unsafe {
                            &MemOperand::new1(Reg::R0.into(), if transfer.add() { 4 } else { -4 }, if transfer.pre() { AddrMode_PreIndex } else { AddrMode_PostIndex })
                        });
                    } else {
                        let func = match (transfer.pre(), transfer.add()) {
                            (false, false) => <MacroAssembler as MasmLdmda3<Reg, WriteBack, RegReserve>>::ldmda3,
                            (false, true) => <MacroAssembler as MasmLdm3<_, _, _>>::ldm3,
                            (true, false) => <MacroAssembler as MasmLdmdb3<_, _, _>>::ldmdb3,
                            (true, true) => <MacroAssembler as MasmLdmib3<_, _, _>>::ldmib3,
                        };
                        func(block_asm, Reg::R0, WriteBack::yes(), usable_regs);
                    }

                    block_asm.restore_guest_regs_ptr();
                    for (guest_reg, usable_reg) in guest_regs.into_iter().zip(usable_regs) {
                        block_asm.store_guest_reg(usable_reg, guest_reg);
                    }
                }
                op1 -= guest_regs;
                remaining_op1 -= len;
            }
            debug_assert!(op1.is_empty());

            if transfer.write_back() {
                block_asm.sub5(FlagsUpdate_DontCare, Cond::AL, Reg::R0, Reg::R0, &Reg::R1.into());
                block_asm.store_guest_reg(Reg::R0, op0);
            }

            block_asm.nop0();
            let fast_mem_end = block_asm.get_cursor_offset();
            let fast_mem_size = fast_mem_end - fast_mem_start;

            let (slow_mem_length, step_size) = if block_asm.thumb {
                (SLOW_MEM_MULTIPLE_LENGTH_THUMB, 2)
            } else {
                (SLOW_MEM_MULTIPLE_LENGTH_ARM, 4)
            };
            for _ in (fast_mem_size as usize..slow_mem_length).step_by(step_size) {
                block_asm.nop0();
            }
        } else {
            let mut pre = transfer.pre();
            if !transfer.add() {
                pre = !pre;
            }

            let func = match (inst.op.is_write_mem_transfer(), transfer.write_back(), !transfer.add()) {
                (false, false, false) => inst_mem_handler_multiple_slow::<CPU, false, false, false> as *const (),
                (false, false, true) => inst_mem_handler_multiple_slow::<CPU, false, false, true> as _,
                (false, true, false) => inst_mem_handler_multiple_slow::<CPU, false, true, false> as _,
                (false, true, true) => inst_mem_handler_multiple_slow::<CPU, false, true, true> as _,
                (true, false, false) => inst_mem_handler_multiple_slow::<CPU, true, false, false> as _,
                (true, false, true) => inst_mem_handler_multiple_slow::<CPU, true, false, true> as _,
                (true, true, false) => inst_mem_handler_multiple_slow::<CPU, true, true, false> as _,
                (true, true, true) => inst_mem_handler_multiple_slow::<CPU, true, true, true> as _,
            };

            let params = InstMemMultipleParams::new(op1.0 as u16, u4::new(op1.len() as u8), u4::new(op0 as u8), pre, transfer.user(), u6::new(0));

            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &u32::from(params).into());
            let mut pc = block_asm.current_pc;
            if block_asm.thumb {
                pc |= 1;
            }
            block_asm.ldr2(Reg::R1, pc);
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R2, &self.jit_buf.insts_cycle_counts[inst_index].into());

            block_asm.call(func);
        }

        block_asm.restore_tmp_regs(next_live_regs);
        block_asm.reload_active_guest_regs();
    }
}

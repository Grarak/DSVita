use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::jit::assembler::block_asm::{BlockAsm, CPSR_TMP_REG, GUEST_REGS_PTR_REG};
use crate::jit::assembler::vixl::vixl::{AddrMode_PostIndex, AddrMode_PreIndex, FlagsUpdate, FlagsUpdate_DontCare, FlagsUpdate_LeaveFlags, MemOperand, WriteBack};
use crate::jit::assembler::vixl::{
    vixl, MacroAssembler, MasmAdd5, MasmAnd5, MasmBic5, MasmCmp2, MasmCmp3, MasmLdm3, MasmLdmda3, MasmLdmdb3, MasmLdmib3, MasmLdr2, MasmLdr3, MasmLdrb2, MasmLdrh2, MasmLdrsb2, MasmLdrsh2, MasmLsl5,
    MasmMov4, MasmNop, MasmRor5, MasmStm3, MasmStmda3, MasmStmdb3, MasmStmib3, MasmStr2, MasmStr3, MasmStrb2, MasmStrh2, MasmSub5,
};
use crate::jit::jit_asm::JitAsm;
use crate::jit::jit_memory::{JitMemory, SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM, SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM};
use crate::jit::op::{MultipleTransfer, Op};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use std::cmp::{max, min};
use std::mem;
use CpuType::{ARM7, ARM9};

macro_rules! get_read_func {
    ($size:expr, $signed:expr) => {
        match $size {
            1 => {
                if $signed {
                    <MacroAssembler as MasmLdrsb2<Reg, &MemOperand>>::ldrsb2
                } else {
                    <MacroAssembler as MasmLdrb2<Reg, &MemOperand>>::ldrb2
                }
            }
            2 => {
                if $signed {
                    <MacroAssembler as MasmLdrsh2<_, _>>::ldrsh2
                } else {
                    <MacroAssembler as MasmLdrh2<_, _>>::ldrh2
                }
            }
            4 => <MacroAssembler as MasmLdr2<_, _>>::ldr2,
            _ => unreachable!(),
        }
    };
}

impl JitAsm<'_> {
    fn pad_nop(current_length: usize, max_length: usize, block_asm: &mut BlockAsm) {
        for _ in (current_length..max_length).step_by(if block_asm.thumb { 2 } else { 4 }) {
            block_asm.nop0();
        }
    }

    fn emit_align_addr(flag_update: u32, dest_reg: Reg, src_reg: Reg, size: u8, block_asm: &mut BlockAsm) {
        if block_asm.thumb {
            block_asm.ldr2(dest_reg, !(0xF0000000 | (size as u32 - 1)));
            block_asm.and5(flag_update, Cond::AL, dest_reg, dest_reg, &src_reg.into());
        } else {
            block_asm.bic5(flag_update, Cond::AL, dest_reg, src_reg, &(0xF0000000 | (size as u32 - 1)).into());
        }
    }

    fn emit_fast_single_write_transfer<F: FnOnce(&Self, &mut BlockAsm)>(&mut self, flag_update: u32, op0: Reg, op0_next: Reg, tmp_reg: Reg, size: u8, metadata_emitter: F, block_asm: &mut BlockAsm) {
        let is_64bit = size == 8;
        let size = if is_64bit { 4 } else { size };

        Self::emit_align_addr(flag_update, tmp_reg, Reg::R2, size, block_asm);

        let func = match size {
            1 => <MacroAssembler as MasmStrb2<Reg, &MemOperand>>::strb2,
            2 => <MacroAssembler as MasmStrh2<_, _>>::strh2,
            4 => <MacroAssembler as MasmStr2<_, _>>::str2,
            _ => unreachable!(),
        };

        block_asm.mov4(flag_update, Cond::AL, Reg::LR, &(self.cpu.mmu_tcm_addr() as u32).into());

        metadata_emitter(self, block_asm);

        let mem_operand = MemOperand::reg_offset2(tmp_reg, Reg::LR);
        func(block_asm, op0, &mem_operand);
        if is_64bit {
            block_asm.add5(flag_update, Cond::AL, Reg::R2, Reg::R2, &4.into());
            Self::emit_align_addr(flag_update, tmp_reg, Reg::R2, 4, block_asm);

            func(block_asm, op0_next, &mem_operand);
        }
    }

    fn emit_fast_single_read_transfer<F: FnOnce(&Self, &mut BlockAsm)>(
        &mut self,
        flag_update: u32,
        op0: Reg,
        op0_next: Reg,
        tmp_reg: Reg,
        size: u8,
        signed: bool,
        metadata_emitter: F,
        block_asm: &mut BlockAsm,
    ) {
        debug_assert!(!signed || size == 1 || size == 2);

        let is_64bit = size == 8;
        let size = if is_64bit { 4 } else { size };

        Self::emit_align_addr(flag_update, tmp_reg, Reg::R2, size, block_asm);

        let func = get_read_func!(size, signed);

        block_asm.mov4(flag_update, Cond::AL, Reg::LR, &(self.cpu.mmu_tcm_addr() as u32).into());

        metadata_emitter(self, block_asm);

        let mem_operand = MemOperand::reg_offset2(tmp_reg, Reg::LR);
        func(block_asm, op0, &mem_operand);
        if size == 4 {
            block_asm.lsl5(flag_update, Cond::AL, tmp_reg, Reg::R2, &3.into());
            block_asm.ror5(flag_update, Cond::AL, op0, op0, &tmp_reg.into());
        }
        if is_64bit {
            block_asm.add5(flag_update, Cond::AL, Reg::R2, Reg::R2, &4.into());
            Self::emit_align_addr(flag_update, tmp_reg, Reg::R2, 4, block_asm);

            func(block_asm, op0_next, &mem_operand);
            block_asm.lsl5(flag_update, Cond::AL, tmp_reg, Reg::R2, &3.into());
            block_asm.ror5(flag_update, Cond::AL, op0_next, op0_next, &tmp_reg.into());
        }
    }

    pub fn emit_single_transfer(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        let transfer = match inst.op {
            Op::Ldr(transfer) | Op::LdrT(transfer) | Op::Str(transfer) | Op::StrT(transfer) => transfer,
            _ => unreachable!(),
        };

        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let op0_next = Reg::from(op0 as u8 + 1);
        let op1 = inst.operands()[1].as_reg_no_shift().unwrap();
        let op2 = &inst.operands()[2];

        let op0_mapped = block_asm.get_guest_map(op0);

        let size = 1 << transfer.size();
        let is_64bit = size == 8;

        let mut value_reg = op0_mapped;
        let next_value_reg = if is_64bit { block_asm.get_guest_map(Reg::from(op0 as u8 + 1)) } else { Reg::None };

        let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
        let save_cpsr = block_asm.dirty_guest_regs.is_reserved(Reg::CPSR) || next_live_regs.is_reserved(Reg::CPSR);
        let flag_update = if save_cpsr { FlagsUpdate_LeaveFlags } else { FlagsUpdate_DontCare };

        let mut dirty_guest_regs = block_asm.get_dirty_guest_regs();
        if save_cpsr {
            dirty_guest_regs += Reg::CPSR;
        }

        block_asm.ensure_emit_for(64);
        let fast_mem_start = if !inst.op.is_write_mem_transfer() && op1 == Reg::PC && !transfer.write_back() && op2.as_imm().is_some() {
            let imm = op2.as_imm().unwrap();
            let pc = if block_asm.thumb { block_asm.current_pc + 4 } else { block_asm.current_pc + 8 };
            let mut imm_addr = if transfer.add() { pc + imm } else { pc - imm };

            if block_asm.thumb {
                imm_addr &= !0x3;
            }

            block_asm.ldr2(Reg::R2, imm_addr);

            let func = get_read_func!(size, transfer.signed());

            block_asm.ensure_emit_for(64);
            let fast_mem_start = block_asm.get_cursor_offset();

            block_asm.ldr2(Reg::R1, (imm_addr & !(0xF0000000 | (size as u32 - 1))) + self.cpu.mmu_tcm_addr() as u32);

            block_asm.guest_inst_metadata(
                self.jit_buf.insts_cycle_counts[inst_index],
                &self.jit_buf.insts[inst_index],
                fast_mem_start,
                value_reg,
                dirty_guest_regs,
            );
            let mem_operand = MemOperand::reg(Reg::R1);
            func(block_asm, value_reg, &mem_operand);
            let shift = (imm_addr & 0x3) << 3;
            if !is_64bit && size == 4 && shift != 0 {
                block_asm.ror5(flag_update, Cond::AL, value_reg, value_reg, &shift.into());
            }
            if is_64bit {
                func(block_asm, next_value_reg, &MemOperand::reg_offset(Reg::R1, 4));
            }

            fast_mem_start
        } else {
            let op1_mapped = block_asm.get_guest_map(op1);
            let op2_mapped = block_asm.get_guest_operand_map(op2);

            if transfer.add() {
                block_asm.add5(flag_update, Cond::AL, Reg::R1, op1_mapped, &op2_mapped);
            } else {
                block_asm.sub5(flag_update, Cond::AL, Reg::R1, op1_mapped, &op2_mapped);
            }

            if transfer.pre() {
                block_asm.mov4(flag_update, Cond::AL, Reg::R2, &Reg::R1.into());
            } else {
                block_asm.mov4(flag_update, Cond::AL, Reg::R2, &op1_mapped.into());
            }

            if inst.op.is_write_mem_transfer() && !block_asm.thumb && op0 == Reg::PC {
                // When op0 is PC, it's read as PC+12
                block_asm.add5(flag_update, Cond::AL, Reg::R0, value_reg, &4.into());
                value_reg = Reg::R0;
            }

            if transfer.write_back() && (inst.op.is_write_mem_transfer() || (op0 != op1 && (!is_64bit || op0_next != op1))) {
                if inst.op.is_write_mem_transfer() && value_reg != Reg::R0 {
                    block_asm.mov4(flag_update, Cond::AL, Reg::R0, &value_reg.into());
                    value_reg = Reg::R0;
                }
                block_asm.mov4(flag_update, Cond::AL, op1_mapped, &Reg::R1.into());
                dirty_guest_regs += op1;
            }

            block_asm.ensure_emit_for(64);
            let fast_mem_start = block_asm.get_cursor_offset();

            let metadata_emitter = |asm: &Self, block_asm: &mut BlockAsm| {
                block_asm.guest_inst_metadata(asm.jit_buf.insts_cycle_counts[inst_index], &asm.jit_buf.insts[inst_index], fast_mem_start, value_reg, dirty_guest_regs)
            };

            if inst.op.is_write_mem_transfer() {
                self.emit_fast_single_write_transfer(flag_update, value_reg, next_value_reg, Reg::R1, size, metadata_emitter, block_asm);
            } else {
                self.emit_fast_single_read_transfer(flag_update, value_reg, next_value_reg, Reg::R1, size, transfer.signed(), metadata_emitter, block_asm);
            }

            fast_mem_start
        };

        let fast_mem_end = block_asm.get_cursor_offset();
        let fast_mem_size = fast_mem_end - fast_mem_start;
        let slow_mem_size = JitMemory::get_slow_mem_length(self.jit_buf.insts[inst_index].op);
        Self::pad_nop(fast_mem_size as usize, slow_mem_size, block_asm);
        block_asm.set_fast_mem_size_last(max(fast_mem_size, slow_mem_size as u32) as u16);
    }

    fn emit_multiple_transfer_load_store_guest_regs(flag_update: FlagsUpdate, guest_regs: RegReserve, usable_regs: RegReserve, is_write: bool, block_asm: &mut BlockAsm) {
        debug_assert_eq!(guest_regs.len(), usable_regs.len());
        let mut size = 0;
        let mut current_reg = Reg::None;
        let mut current_usable_regs = RegReserve::new();

        let flush = |size: &mut u8, current_reg: &mut Reg, current_usable_regs: &mut RegReserve, block_asm: &mut BlockAsm| {
            if *size == 0 {
                return;
            }

            let start_reg = Reg::from(*current_reg as u8 + 1 - *size);
            let use_multiple = (start_reg == Reg::R0 && *size >= 3) || *size >= 4;
            if use_multiple {
                if start_reg != Reg::R0 {
                    block_asm.add5(flag_update, Cond::AL, GUEST_REGS_PTR_REG, GUEST_REGS_PTR_REG, &(start_reg as u32 * 4).into());
                }
                if is_write {
                    block_asm.ldm3(GUEST_REGS_PTR_REG, WriteBack::no(), *current_usable_regs);
                } else {
                    block_asm.stm3(GUEST_REGS_PTR_REG, WriteBack::no(), *current_usable_regs);
                }
                if start_reg != Reg::R0 {
                    block_asm.sub5(flag_update, Cond::AL, GUEST_REGS_PTR_REG, GUEST_REGS_PTR_REG, &(start_reg as u32 * 4).into());
                }
            } else {
                for i in 0..*size {
                    let guest_reg = Reg::from(start_reg as u8 + i);
                    let usable_reg = current_usable_regs.peek().unwrap();
                    *current_usable_regs -= usable_reg;
                    if is_write {
                        block_asm.load_guest_reg(usable_reg, guest_reg);
                    } else {
                        block_asm.store_guest_reg(usable_reg, guest_reg);
                    }
                }
            }

            *size = 0;
            *current_reg = Reg::None;
            *current_usable_regs = RegReserve::new();
        };

        for (guest_reg, usable_reg) in guest_regs.into_iter().zip(usable_regs) {
            if current_reg != Reg::None && guest_reg as u8 != current_reg as u8 + 1 {
                flush(&mut size, &mut current_reg, &mut current_usable_regs, block_asm);
            }
            current_reg = guest_reg;
            current_usable_regs += usable_reg;
            size += 1;
        }

        flush(&mut size, &mut current_reg, &mut current_usable_regs, block_asm);
    }

    fn emit_multiple_transfer_fast(&mut self, inst_index: usize, basic_block_index: usize, transfer: MultipleTransfer, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let op1 = inst.operands()[1].as_reg_list().unwrap();

        let op0_mapped = block_asm.get_guest_map(op0);
        let op1_len = op1.len();

        let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
        let save_cpsr = block_asm.dirty_guest_regs.is_reserved(Reg::CPSR) || next_live_regs.is_reserved(Reg::CPSR);
        let mut flag_update = if save_cpsr { FlagsUpdate_LeaveFlags } else { FlagsUpdate_DontCare };
        let user = transfer.user() && !op1.is_reserved(Reg::PC);

        let mut dirty_guest_regs = block_asm.get_dirty_guest_regs();
        if save_cpsr {
            if user {
                block_asm.store_guest_cpsr_reg(CPSR_TMP_REG);
                flag_update = FlagsUpdate_DontCare;
            } else {
                dirty_guest_regs += Reg::CPSR;
            }
        }

        block_asm.ensure_emit_for(64);
        let fast_mem_start = block_asm.get_cursor_offset();
        let guest_inst_metadata_start = block_asm.get_guest_inst_metadata_len();

        Self::emit_align_addr(flag_update, Reg::R0, op0_mapped, 4, block_asm);

        let mut write_back = transfer.write_back();
        if write_back && op1.is_reserved(op0) {
            match self.cpu {
                ARM9 => {
                    if inst.op.is_write_mem_transfer() {
                        // always store OLD base
                    } else {
                        // writeback if Rb is “the ONLY register, or NOT the LAST register” in Rlist
                        if op1.len() > 1 && op1.get_highest_reg() == op0 {
                            write_back = false;
                        }
                    }
                }
                ARM7 => {
                    if inst.op.is_write_mem_transfer() {
                        // Store OLD base if Rb is FIRST entry in Rlist, otherwise store NEW base
                        if op1.get_lowest_reg() != op0 {
                            let func = if transfer.add() {
                                <MacroAssembler as MasmAdd5<FlagsUpdate, Cond, Reg, Reg, &vixl::Operand>>::add5
                            } else {
                                <MacroAssembler as MasmSub5<FlagsUpdate, Cond, Reg, Reg, &vixl::Operand>>::sub5
                            };
                            func(block_asm, flag_update, Cond::AL, op0_mapped, op0_mapped, &(op1_len as u32 * 4).into());
                            write_back = false;
                        }
                    } else {
                        // no writeback
                        write_back = false;
                    }
                }
            }
        }

        if user {
            block_asm.load_guest_reg(Reg::R1, Reg::CPSR);
            block_asm.and5(flag_update, Cond::AL, Reg::R1, Reg::R1, &0x1F.into());
        }

        block_asm.add5(flag_update, Cond::AL, Reg::R0, Reg::R0, &(self.cpu.mmu_tcm_addr() as u32).into());

        let mut usable_regs = reg_reserve!(Reg::R1, Reg::R2, Reg::R12, Reg::LR) + block_asm.get_free_host_regs();
        if user {
            usable_regs -= Reg::R1;
        }

        let mut remaining_op1 = op1;
        let mut remaining_op1_len = op1_len;
        while remaining_op1_len != 0 {
            let len = min(remaining_op1_len, usable_regs.len());
            let mut guest_regs = remaining_op1;
            for _ in 0..remaining_op1_len - len {
                let reg_to_remove = if transfer.add() { guest_regs.get_highest_reg() } else { guest_regs.get_lowest_reg() };
                guest_regs -= reg_to_remove;
            }
            let usable_regs = usable_regs.into_iter().take(len).collect::<RegReserve>();
            let mut guest_regs_multiple_load_store = guest_regs;
            let mut usable_regs_multiple_load_store = usable_regs;
            if inst.op.is_write_mem_transfer() {
                if user {
                    for (guest_reg, usable_reg) in guest_regs.into_iter().zip(usable_regs) {
                        match guest_reg {
                            Reg::R8 | Reg::R9 | Reg::R10 | Reg::R11 | Reg::R12 => {
                                let mapped_reg = block_asm.get_guest_map(guest_reg);

                                // FIQ mode
                                block_asm.cmp2(Reg::R1, &0x11.into());
                                block_asm.ldr3(
                                    Cond::EQ,
                                    usable_reg,
                                    &MemOperand::reg_offset(GUEST_REGS_PTR_REG, mem::offset_of!(ThreadRegs, user) as i32 + (guest_reg as i32 - 8) * 4),
                                );
                                if mapped_reg != Reg::None {
                                    block_asm.mov4(flag_update, Cond::NE, usable_reg, &mapped_reg.into());
                                } else {
                                    block_asm.load_guest_reg_cond(Cond::NE, usable_reg, guest_reg);
                                }
                            }
                            Reg::SP | Reg::LR => {
                                let mapped_reg = block_asm.get_guest_map(guest_reg);

                                block_asm.cmp2(Reg::R1, &0x1F.into());
                                block_asm.cmp3(Cond::NE, Reg::R1, &0x10.into());
                                block_asm.ldr3(
                                    Cond::NE,
                                    usable_reg,
                                    &MemOperand::reg_offset(GUEST_REGS_PTR_REG, mem::offset_of!(ThreadRegs, user) as i32 + (guest_reg as i32 - 8) * 4),
                                );
                                if mapped_reg != Reg::None {
                                    block_asm.mov4(flag_update, Cond::EQ, usable_reg, &mapped_reg.into());
                                } else {
                                    block_asm.load_guest_reg_cond(Cond::EQ, usable_reg, guest_reg);
                                }
                            }
                            _ => {
                                let mapped_reg = block_asm.get_guest_map(guest_reg);
                                if mapped_reg != Reg::None {
                                    block_asm.mov4(flag_update, Cond::AL, usable_reg, &mapped_reg.into());
                                } else {
                                    block_asm.load_guest_reg(usable_reg, guest_reg);
                                }
                            }
                        }
                    }
                } else {
                    for (guest_reg, usable_reg) in guest_regs.into_iter().zip(usable_regs) {
                        if guest_reg == Reg::PC {
                            let pc = block_asm.current_pc + (4 << (!block_asm.thumb as u32));
                            block_asm.ldr2(usable_reg, pc);
                            guest_regs_multiple_load_store -= Reg::PC;
                            usable_regs_multiple_load_store -= usable_reg;
                        } else {
                            let mapped_reg = block_asm.get_guest_map(guest_reg);
                            if mapped_reg != Reg::None {
                                block_asm.mov4(flag_update, Cond::AL, usable_reg, &mapped_reg.into());
                                guest_regs_multiple_load_store -= guest_reg;
                                usable_regs_multiple_load_store -= usable_reg;
                            }
                        }
                    }
                    Self::emit_multiple_transfer_load_store_guest_regs(flag_update, guest_regs_multiple_load_store, usable_regs_multiple_load_store, true, block_asm);
                }

                block_asm.guest_inst_metadata(self.jit_buf.insts_cycle_counts[inst_index], inst, fast_mem_start, op0, dirty_guest_regs);

                if remaining_op1_len == 1 {
                    let usable_reg = usable_regs.peek().unwrap();
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
            } else {
                block_asm.guest_inst_metadata(self.jit_buf.insts_cycle_counts[inst_index], inst, fast_mem_start, op0, dirty_guest_regs);

                if remaining_op1_len == 1 {
                    let usable_reg = usable_regs.peek().unwrap();
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

                if user {
                    for (guest_reg, usable_reg) in guest_regs.into_iter().zip(usable_regs) {
                        match guest_reg {
                            Reg::R8 | Reg::R9 | Reg::R10 | Reg::R11 | Reg::R12 => {
                                let mapped_reg = block_asm.get_guest_map(guest_reg);

                                // FIQ mode
                                block_asm.cmp2(Reg::R1, &0x11.into());
                                block_asm.str3(
                                    Cond::EQ,
                                    usable_reg,
                                    &MemOperand::reg_offset(GUEST_REGS_PTR_REG, mem::offset_of!(ThreadRegs, user) as i32 + (guest_reg as i32 - 8) * 4),
                                );
                                if mapped_reg != Reg::None {
                                    block_asm.mov4(flag_update, Cond::NE, mapped_reg, &usable_reg.into());
                                }
                                block_asm.store_guest_reg_cond(Cond::NE, usable_reg, guest_reg);
                            }
                            Reg::SP | Reg::LR => {
                                let mapped_reg = block_asm.get_guest_map(guest_reg);

                                block_asm.cmp2(Reg::R1, &0x1F.into());
                                block_asm.cmp3(Cond::NE, Reg::R1, &0x10.into());
                                block_asm.str3(
                                    Cond::NE,
                                    usable_reg,
                                    &MemOperand::reg_offset(GUEST_REGS_PTR_REG, mem::offset_of!(ThreadRegs, user) as i32 + (guest_reg as i32 - 8) * 4),
                                );
                                if mapped_reg != Reg::None {
                                    block_asm.mov4(flag_update, Cond::EQ, mapped_reg, &usable_reg.into());
                                }
                                block_asm.store_guest_reg_cond(Cond::EQ, usable_reg, guest_reg);
                            }
                            _ => {
                                let mapped_reg = block_asm.get_guest_map(guest_reg);
                                if mapped_reg != Reg::None {
                                    block_asm.mov4(flag_update, Cond::AL, mapped_reg, &usable_reg.into());
                                }
                                block_asm.store_guest_reg(usable_reg, guest_reg);
                            }
                        }
                    }
                } else {
                    for (guest_reg, usable_reg) in guest_regs.into_iter().zip(usable_regs) {
                        let mapped_reg = block_asm.get_guest_map(guest_reg);
                        if mapped_reg != Reg::None {
                            block_asm.mov4(flag_update, Cond::AL, mapped_reg, &usable_reg.into());
                            guest_regs_multiple_load_store -= guest_reg;
                            usable_regs_multiple_load_store -= usable_reg;
                        }
                    }
                    Self::emit_multiple_transfer_load_store_guest_regs(flag_update, guest_regs_multiple_load_store, usable_regs_multiple_load_store, false, block_asm);
                }
            }
            remaining_op1 -= guest_regs;
            remaining_op1_len -= len;
        }
        debug_assert!(remaining_op1.is_empty());

        if write_back {
            block_asm.sub5(flag_update, Cond::AL, op0_mapped, Reg::R0, &(self.cpu.mmu_tcm_addr() as u32).into());
        }

        let fast_mem_end = block_asm.get_cursor_offset();
        let fast_mem_size = fast_mem_end - fast_mem_start;
        let slow_mem_size = JitMemory::get_slow_mem_length(inst.op);
        Self::pad_nop(fast_mem_size as usize, slow_mem_size, block_asm);
        block_asm.set_fast_mem_size(guest_inst_metadata_start, max(fast_mem_size, slow_mem_size as u32) as u16);

        if save_cpsr && user {
            block_asm.load_guest_cpsr_reg(CPSR_TMP_REG);
        }
    }

    pub fn emit_multiple_transfer(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let op1 = inst.operands()[1].as_reg_list().unwrap();

        let transfer = match inst.op {
            Op::Ldm(transfer) | Op::LdmT(transfer) | Op::Stm(transfer) | Op::StmT(transfer) => transfer,
            _ => unreachable!(),
        };

        if op1.is_empty() {
            if self.cpu == ARM7 {
                todo!()
            }
            let op0_mapped = block_asm.get_guest_map(op0);
            let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
            let save_cpsr = block_asm.dirty_guest_regs.is_reserved(Reg::CPSR) || next_live_regs.is_reserved(Reg::CPSR);
            let flag_update = if save_cpsr { FlagsUpdate_LeaveFlags } else { FlagsUpdate_DontCare };

            if inst.op.is_write_mem_transfer() {
                block_asm.sub5(flag_update, Cond::AL, op0_mapped, op0_mapped, &0x40.into());
            } else {
                block_asm.add5(flag_update, Cond::AL, op0_mapped, op0_mapped, &0x40.into());
            }
        } else {
            self.emit_multiple_transfer_fast(inst_index, basic_block_index, transfer, block_asm);
        }
    }

    pub fn emit_swp(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        let operands = inst.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let read_reg = block_asm.get_guest_map(op0);
        let value_reg = block_asm.get_guest_map(operands[1].as_reg_no_shift().unwrap());
        let addr_reg = block_asm.get_guest_map(operands[2].as_reg_no_shift().unwrap());

        let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
        let save_cpsr = block_asm.dirty_guest_regs.is_reserved(Reg::CPSR) || next_live_regs.is_reserved(Reg::CPSR);
        let flag_update = if save_cpsr { FlagsUpdate_LeaveFlags } else { FlagsUpdate_DontCare };

        let mut dirty_guest_regs = block_asm.get_dirty_guest_regs();
        if save_cpsr {
            dirty_guest_regs += Reg::CPSR;
        }

        block_asm.mov4(flag_update, Cond::AL, Reg::R1, &value_reg.into());
        block_asm.mov4(flag_update, Cond::AL, Reg::R2, &addr_reg.into());

        block_asm.ensure_emit_for(64);
        let fast_mem_start = block_asm.get_cursor_offset();

        let size = if inst.op == Op::Swpb { 1 } else { 4 };
        self.emit_fast_single_read_transfer(
            flag_update,
            Reg::R0,
            Reg::None,
            Reg::R12,
            size,
            false,
            |asm, block_asm| block_asm.guest_inst_metadata(asm.jit_buf.insts_cycle_counts[inst_index], &asm.jit_buf.insts[inst_index], fast_mem_start, Reg::R0, dirty_guest_regs),
            block_asm,
        );

        let fast_mem_end = block_asm.get_cursor_offset();
        let fast_mem_size = fast_mem_end - fast_mem_start;
        Self::pad_nop(fast_mem_size as usize, SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM, block_asm);
        block_asm.set_fast_mem_size_last(max(fast_mem_size, SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM as u32) as u16);

        block_asm.mov4(flag_update, Cond::AL, Reg::R1, &value_reg.into());
        block_asm.mov4(flag_update, Cond::AL, Reg::R2, &addr_reg.into());
        block_asm.mov4(flag_update, Cond::AL, read_reg, &Reg::R0.into());
        block_asm.store_guest_reg(read_reg, op0);
        block_asm.mov4(flag_update, Cond::AL, Reg::R0, &Reg::R1.into());

        block_asm.ensure_emit_for(64);
        let fast_mem_start = block_asm.get_cursor_offset();

        self.emit_fast_single_write_transfer(
            flag_update,
            Reg::R1,
            Reg::None,
            Reg::R12,
            size,
            |asm, block_asm| block_asm.guest_inst_metadata(asm.jit_buf.insts_cycle_counts[inst_index], &asm.jit_buf.insts[inst_index], fast_mem_start, Reg::R1, dirty_guest_regs),
            block_asm,
        );
        let fast_mem_end = block_asm.get_cursor_offset();
        let fast_mem_size = fast_mem_end - fast_mem_start;
        Self::pad_nop(fast_mem_size as usize, SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM, block_asm);
        block_asm.set_fast_mem_size_last(max(fast_mem_size, SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM as u32) as u16);
    }
}

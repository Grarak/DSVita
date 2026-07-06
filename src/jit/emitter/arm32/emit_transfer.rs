use crate::core::memory::regions::VRAM_OFFSET;
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::jit::assembler::block_asm::{BlockAsm, CPSR_TMP_REG, GUEST_REGS_PTR_REG};
use crate::jit::jit_asm::JitAsm;
use crate::jit::jit_memory::{JitMemory, SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM, SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM};
use crate::jit::op::{MultipleTransfer, Op};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use std::cmp::{max, min};
use std::mem;
use vixl::{
    AddrMode_PostIndex, AddrMode_PreIndex, FlagsUpdate, FlagsUpdate_DontCare, FlagsUpdate_LeaveFlags, MacroAssembler, MasmAdd5, MasmAnd5, MasmBic5, MasmCmp2, MasmCmp3, MasmLdm3, MasmLdmda3,
    MasmLdmdb3, MasmLdmib3, MasmLdr2, MasmLdr3, MasmLdrb2, MasmLdrh2, MasmLdrsb2, MasmLdrsh2, MasmLsl5, MasmMov4, MasmNop, MasmRor5, MasmStm3, MasmStmda3, MasmStmdb3, MasmStmib3, MasmStr2, MasmStr3,
    MasmStrb2, MasmStrh2, MasmSub5, MemOperand, WriteBack,
};
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

macro_rules! get_write_func {
    ($size:expr) => {
        match $size {
            1 => <MacroAssembler as MasmStrb2<Reg, &MemOperand>>::strb2,
            2 => <MacroAssembler as MasmStrh2<_, _>>::strh2,
            4 => <MacroAssembler as MasmStr2<_, _>>::str2,
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

        let func = get_write_func!(size);

        block_asm.mov4(flag_update, Cond::AL, Reg::LR, &(self.cpu.mmu_tcm_addr() as u32).into());

        metadata_emitter(self, block_asm);

        let mem_operand = (tmp_reg, Reg::LR).into();
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

        block_asm.mov4(flag_update, Cond::AL, Reg::R0, &(self.cpu.mmu_tcm_addr() as u32).into());

        metadata_emitter(self, block_asm);

        let mem_operand = (tmp_reg, Reg::R0).into();
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

        let is_write = inst.op.is_write_mem_transfer();

        let fast_mem_start = match inst.imm_transfer_addr(block_asm.current_pc) {
            Some(imm_addr) => {
                let consider_slow_mem = block_asm.current_pc & 0xFF000000 == VRAM_OFFSET;

                if consider_slow_mem {
                    block_asm.ldr2(Reg::R2, imm_addr);
                    block_asm.ensure_emit_for(64);
                } else if !is_write && size == 4 && self.analyzer.can_imm_load(imm_addr) {
                    let imm_value = match self.cpu {
                        ARM9 => self.emu.mem_read::<{ ARM9 }, u32>(imm_addr),
                        ARM7 => self.emu.mem_read::<{ ARM7 }, u32>(imm_addr),
                    };
                    block_asm.ldr2(value_reg, imm_value);
                    return;
                }

                let func = if is_write { get_write_func!(size) } else { get_read_func!(size, transfer.signed()) };

                let fast_mem_start = block_asm.get_cursor_offset();

                let aligned_addr = imm_addr & !(0xF0000000 | (size as u32 - 1));
                // Don't query write offset here, we don't want to invalidate the jit block
                let shm_offset = match self.cpu {
                    ARM9 => self.emu.get_shm_offset::<{ ARM9 }, true, false>(aligned_addr),
                    ARM7 => self.emu.get_shm_offset::<{ ARM7 }, true, false>(aligned_addr),
                };
                let host_addr = if is_write && shm_offset != 0 {
                    (self.emu.mem.shm.as_mut_ptr() as usize + shm_offset) as u32
                } else {
                    aligned_addr + self.cpu.mmu_tcm_addr() as u32
                };
                block_asm.ldr2(Reg::R1, host_addr);

                if consider_slow_mem {
                    block_asm.guest_inst_metadata(
                        self.jit_buf.insts_cycle_counts[inst_index],
                        &self.jit_buf.insts[inst_index],
                        fast_mem_start,
                        value_reg,
                        dirty_guest_regs,
                    );
                }
                let mem_operand = Reg::R1.into();
                func(block_asm, value_reg, &mem_operand);

                if !is_write {
                    let shift = (imm_addr & 0x3) << 3;
                    if !is_64bit && size == 4 && shift != 0 {
                        block_asm.ror5(flag_update, Cond::AL, value_reg, value_reg, &shift.into());
                    }
                }
                if is_64bit {
                    func(block_asm, next_value_reg, &(Reg::R1, 4).into());
                }

                if !consider_slow_mem {
                    return;
                }

                fast_mem_start
            }
            _ => {
                let op1_mapped = block_asm.get_guest_map(op1);
                let op2_mapped = block_asm.get_guest_operand_map(op2);

                let addr_reg = if transfer.pre() { Reg::R2 } else { Reg::R1 };
                if transfer.add() {
                    block_asm.add5(flag_update, Cond::AL, addr_reg, op1_mapped, &op2_mapped);
                } else {
                    block_asm.sub5(flag_update, Cond::AL, addr_reg, op1_mapped, &op2_mapped);
                }

                if !transfer.pre() {
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
                    block_asm.mov4(flag_update, Cond::AL, op1_mapped, &addr_reg.into());
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
            }
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
                block_asm.store_guest_cpsr_reg(FlagsUpdate_DontCare, CPSR_TMP_REG);
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

            let mut direct_regs = false;
            let mut usable_regs = usable_regs.into_iter().take(len).collect::<RegReserve>();
            if !user {
                let mut guest_regs_in_order = true;
                let mut prev_mapped_reg = Reg::None;
                let mut direct_usable_regs = reg_reserve!();
                for guest_reg in guest_regs {
                    let mapped_reg = block_asm.get_guest_map(guest_reg);
                    if mapped_reg == Reg::None || (prev_mapped_reg != Reg::None && mapped_reg < prev_mapped_reg) {
                        guest_regs_in_order = false;
                        break;
                    }
                    direct_usable_regs += mapped_reg;
                    prev_mapped_reg = mapped_reg;
                }
                if guest_regs_in_order {
                    direct_regs = true;
                    usable_regs = direct_usable_regs;
                }
            }

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
                                    &(GUEST_REGS_PTR_REG, mem::offset_of!(ThreadRegs, user) as i32 + (guest_reg as i32 - 8) * 4).into(),
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
                                    &(GUEST_REGS_PTR_REG, mem::offset_of!(ThreadRegs, user) as i32 + (guest_reg as i32 - 8) * 4).into(),
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
                    let pc = block_asm.current_pc + (4 << (!block_asm.thumb as u32));
                    if direct_regs {
                        if guest_regs.is_reserved(Reg::PC) {
                            let reg = block_asm.get_guest_map(Reg::PC);
                            block_asm.ldr2(reg, pc);
                        }
                    } else {
                        for (guest_reg, usable_reg) in guest_regs.into_iter().zip(usable_regs) {
                            if guest_reg == Reg::PC {
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
                                    &(GUEST_REGS_PTR_REG, mem::offset_of!(ThreadRegs, user) as i32 + (guest_reg as i32 - 8) * 4).into(),
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
                                    &(GUEST_REGS_PTR_REG, mem::offset_of!(ThreadRegs, user) as i32 + (guest_reg as i32 - 8) * 4).into(),
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
                } else if !direct_regs {
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

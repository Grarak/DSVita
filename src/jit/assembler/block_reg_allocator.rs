use crate::fixed_fifo::FixedFifo;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm};
use crate::jit::assembler::block_asm::BLOCK_LOG;
use crate::jit::assembler::block_inst::{BlockInst, TransferOp};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use crate::utils::HeapMem;
use crate::IS_DEBUG;
use std::intrinsics::unlikely;

const DEBUG: bool = false;

pub const ALLOCATION_REGS: RegReserve = reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11);
const SCRATCH_REGS: RegReserve = reg_reserve!(Reg::R0, Reg::R1, Reg::R2, Reg::R3, Reg::R12, Reg::LR);

#[derive(Default)]
pub struct BlockRegAllocator {
    pub global_mapping: HeapMem<Reg, { ANY_REG_LIMIT as usize }>,
    stored_mapping_inputs: HeapMem<Reg, { ANY_REG_LIMIT as usize }>,
    stored_mapping: HeapMem<Reg, { ANY_REG_LIMIT as usize }>, // mappings to real registers
    stored_mapping_reverse: [Option<u16>; Reg::PC as usize],
    spilled: BlockRegSet,
    pub dirty_regs: RegReserve,
    lru_reg: FixedFifo<Reg, 16>,
    last_gen_op: TransferOp,
    last_gen_any_reg: u16,
    last_gen_increment: bool,
    last_gen_regs: RegReserve,
}

impl BlockRegAllocator {
    pub fn init_inputs(&mut self, input_regs: &BlockRegSet) {
        if IS_DEBUG {
            self.stored_mapping_inputs.fill(Reg::None);
        }
        self.stored_mapping.fill(Reg::None);
        self.stored_mapping_reverse.fill(None);
        self.spilled.clear();
        self.lru_reg.clear();

        for any_input_reg in input_regs.iter_any() {
            match self.get_global_mapping(any_input_reg) {
                Reg::None => self.spilled += BlockReg::Any(any_input_reg),
                global_mapping => {
                    self.set_stored_mapping(any_input_reg, global_mapping);
                    self.lru_reg.push_back(global_mapping);
                }
            }
        }

        for reg in SCRATCH_REGS + ALLOCATION_REGS {
            if self.get_stored_mapping_reverse(reg).is_none() {
                self.lru_reg.push_front(reg);
            }
        }
    }

    pub fn for_emit_input(&self, reg: BlockReg) -> Reg {
        match reg {
            BlockReg::Any(any) => {
                let mapped_reg = self.stored_mapping_inputs[any as usize];
                debug_assert_ne!(mapped_reg, Reg::None);
                mapped_reg
            }
            BlockReg::Fixed(reg) => reg,
        }
    }

    pub fn for_emit_output(&self, reg: BlockReg) -> Reg {
        match reg {
            BlockReg::Any(any) => {
                let mapped_reg = *self.get_stored_mapping(any);
                debug_assert_ne!(mapped_reg, Reg::None);
                mapped_reg
            }
            BlockReg::Fixed(reg) => reg,
        }
    }

    fn get_global_mapping(&self, any_reg: u16) -> Reg {
        unsafe { *self.global_mapping.get_unchecked(any_reg as usize) }
    }

    fn get_stored_mapping(&self, any_reg: u16) -> &Reg {
        unsafe { self.stored_mapping.get_unchecked(any_reg as usize) }
    }

    fn get_stored_mapping_mut(&mut self, any_reg: u16) -> &mut Reg {
        unsafe { self.stored_mapping.get_unchecked_mut(any_reg as usize) }
    }

    fn get_stored_mapping_reverse(&self, reg: Reg) -> &Option<u16> {
        unsafe { self.stored_mapping_reverse.get_unchecked(reg as usize) }
    }

    fn get_stored_mapping_reverse_mut(&mut self, reg: Reg) -> &mut Option<u16> {
        unsafe { self.stored_mapping_reverse.get_unchecked_mut(reg as usize) }
    }

    fn gen_pre_handle_spilled_inst(&mut self, any_reg: u16, mapping: Reg, op: TransferOp, opcodes: &mut Vec<u32>) {
        self.dirty_regs += mapping;

        if !self.last_gen_regs.is_empty() {
            if self.last_gen_op != op || self.last_gen_any_reg.abs_diff(any_reg) != 1 {
                self.gen_flush(opcodes);
            } else if self.last_gen_regs.len() == 1 {
                self.last_gen_increment = any_reg > self.last_gen_any_reg;
                if self.last_gen_increment {
                    if mapping as u8 <= self.last_gen_regs.get_highest_reg() as u8 {
                        self.gen_flush(opcodes);
                    }
                } else if mapping as u8 >= self.last_gen_regs.get_lowest_reg() as u8 {
                    self.gen_flush(opcodes);
                }
            } else if (any_reg > self.last_gen_any_reg) != self.last_gen_increment {
                self.gen_flush(opcodes);
            } else if self.last_gen_increment {
                if mapping as u8 <= self.last_gen_regs.get_highest_reg() as u8 {
                    self.gen_flush(opcodes);
                }
            } else if mapping as u8 >= self.last_gen_regs.get_lowest_reg() as u8 {
                self.gen_flush(opcodes);
            }
        }

        self.last_gen_op = op;
        self.last_gen_any_reg = any_reg;
        self.last_gen_regs += mapping;
    }

    fn gen_pre_move_reg(&mut self, dst: Reg, src: Reg, opcodes: &mut Vec<u32>) {
        self.dirty_regs += dst;
        self.gen_flush(opcodes);
        opcodes.push(AluShiftImm::mov_al(dst, src));
    }

    fn gen_flush(&mut self, opcodes: &mut Vec<u32>) {
        if self.last_gen_regs.is_empty() {
            return;
        }
        let len = self.last_gen_regs.len();
        if len > 2 || (self.last_gen_any_reg == len as u16 - 1 && len > 1) {
            if self.last_gen_any_reg != len as u16 - 1 {
                opcodes.push(AluImm::add(
                    Reg::SP,
                    Reg::SP,
                    if self.last_gen_increment {
                        self.last_gen_any_reg as u8 - (len as u8 - 1)
                    } else {
                        self.last_gen_any_reg as u8 + (len as u8 - 1)
                    },
                    15,
                    Cond::AL,
                ));
            }
            opcodes.push(LdmStm::generic(
                Reg::SP,
                self.last_gen_regs,
                self.last_gen_op == TransferOp::Read,
                false,
                self.last_gen_increment,
                false,
                Cond::AL,
            ));
            if self.last_gen_any_reg != len as u16 - 1 {
                opcodes.push(AluImm::sub(
                    Reg::SP,
                    Reg::SP,
                    if self.last_gen_increment {
                        self.last_gen_any_reg as u8 - (len as u8 - 1)
                    } else {
                        self.last_gen_any_reg as u8 + (len as u8 - 1)
                    },
                    15,
                    Cond::AL,
                ));
            }
        } else if len == 2 {
            if self.last_gen_increment {
                opcodes.push(LdrStrImm::generic(
                    self.last_gen_regs.get_highest_reg(),
                    Reg::SP,
                    self.last_gen_any_reg * 4,
                    self.last_gen_op == TransferOp::Read,
                    false,
                    false,
                    true,
                    true,
                    Cond::AL,
                ));
                opcodes.push(LdrStrImm::generic(
                    self.last_gen_regs.get_lowest_reg(),
                    Reg::SP,
                    (self.last_gen_any_reg - 1) * 4,
                    self.last_gen_op == TransferOp::Read,
                    false,
                    false,
                    true,
                    true,
                    Cond::AL,
                ));
            } else {
                opcodes.push(LdrStrImm::generic(
                    self.last_gen_regs.get_lowest_reg(),
                    Reg::SP,
                    self.last_gen_any_reg * 4,
                    self.last_gen_op == TransferOp::Read,
                    false,
                    false,
                    true,
                    true,
                    Cond::AL,
                ));
                opcodes.push(LdrStrImm::generic(
                    self.last_gen_regs.get_highest_reg(),
                    Reg::SP,
                    (self.last_gen_any_reg - 1) * 4,
                    self.last_gen_op == TransferOp::Read,
                    false,
                    false,
                    true,
                    true,
                    Cond::AL,
                ));
            }
        } else {
            opcodes.push(LdrStrImm::generic(
                self.last_gen_regs.get_lowest_reg(),
                Reg::SP,
                self.last_gen_any_reg * 4,
                self.last_gen_op == TransferOp::Read,
                false,
                false,
                true,
                true,
                Cond::AL,
            ));
        }
        self.last_gen_regs.clear();
    }

    fn set_stored_mapping(&mut self, any_reg: u16, reg: Reg) {
        *self.get_stored_mapping_mut(any_reg) = reg;
        *self.get_stored_mapping_reverse_mut(reg) = Some(any_reg);
    }

    fn remove_stored_mapping(&mut self, any_reg: u16) {
        let stored_mapping = *self.get_stored_mapping(any_reg);
        *self.get_stored_mapping_mut(any_reg) = Reg::None;
        *self.get_stored_mapping_reverse_mut(stored_mapping) = None;
    }

    fn pop_lru_reg(&mut self) -> Reg {
        let reg = *self.lru_reg.front();
        self.lru_reg.pop_front();
        self.lru_reg.push_back(reg);
        reg
    }

    fn allocate_reg(&mut self, any_reg: u16, next_live_range: &BlockRegSet, used_regs: &BlockRegSet, opcodes: &mut Vec<u32>) -> Reg {
        let global_mapping = self.get_global_mapping(any_reg);
        if global_mapping != Reg::None && !used_regs.contains(BlockReg::Fixed(global_mapping)) && !next_live_range.contains(BlockReg::Fixed(global_mapping)) {
            let mut use_global_mapping = true;

            if let Some(mapped_reg) = *self.get_stored_mapping_reverse(global_mapping) {
                use_global_mapping = !used_regs.contains(BlockReg::Any(mapped_reg));
                if use_global_mapping {
                    if next_live_range.contains(BlockReg::Any(mapped_reg)) {
                        self.spilled += BlockReg::Any(mapped_reg);
                        self.gen_pre_handle_spilled_inst(mapped_reg, global_mapping, TransferOp::Write, opcodes);
                    }
                    self.remove_stored_mapping(mapped_reg);
                }
            }

            if use_global_mapping {
                self.set_stored_mapping(any_reg, global_mapping);

                let mut new_lru = FixedFifo::new();
                while !self.lru_reg.is_empty() {
                    let reg = *self.lru_reg.front();
                    self.lru_reg.pop_front();
                    if reg != global_mapping {
                        new_lru.push_back(reg);
                    }
                }
                new_lru.push_back(global_mapping);
                self.lru_reg = new_lru;

                return global_mapping;
            }
        }

        loop {
            let reg = self.pop_lru_reg();
            if used_regs.contains(BlockReg::Fixed(reg)) || next_live_range.contains(BlockReg::Fixed(reg)) {
                continue;
            }

            if let Some(mapped_reg) = *self.get_stored_mapping_reverse(reg) {
                if used_regs.contains(BlockReg::Any(mapped_reg)) {
                    continue;
                }

                if next_live_range.contains(BlockReg::Any(mapped_reg)) {
                    self.spilled += BlockReg::Any(mapped_reg);
                    self.gen_pre_handle_spilled_inst(mapped_reg, reg, TransferOp::Write, opcodes);
                }
                self.remove_stored_mapping(mapped_reg);
            }

            if DEBUG && unsafe { BLOCK_LOG } {
                println!("Allocated {reg:?} for {any_reg}");
            }
            self.set_stored_mapping(any_reg, reg);
            return reg;
        }
    }

    fn get_input_reg(&mut self, any_reg: u16, next_live_range: &BlockRegSet, used_regs: &BlockRegSet, opcodes: &mut Vec<u32>) -> Reg {
        match self.get_stored_mapping(any_reg) {
            Reg::None => {
                if self.spilled.contains(BlockReg::Any(any_reg)) {
                    let reg = self.allocate_reg(any_reg, next_live_range, used_regs, opcodes);
                    self.spilled -= BlockReg::Any(any_reg);
                    self.gen_pre_handle_spilled_inst(any_reg, reg, TransferOp::Read, opcodes);
                    reg
                } else {
                    panic!("input reg {any_reg} must be allocated")
                }
            }
            stored_mapping => *stored_mapping,
        }
    }

    fn remove_fixed_reg(&mut self, fixed_reg: Reg, next_live_range: &BlockRegSet, opcodes: &mut Vec<u32>) {
        if DEBUG && unsafe { BLOCK_LOG } {
            println!("Remove fixed reg {fixed_reg:?}");
        }
        if let Some(any_reg) = *self.get_stored_mapping_reverse(fixed_reg) {
            self.remove_stored_mapping(any_reg);
            if DEBUG && unsafe { BLOCK_LOG } {
                println!("Remove stored mapping {any_reg}");
            }
            if next_live_range.contains(BlockReg::Any(any_reg)) {
                if DEBUG && unsafe { BLOCK_LOG } {
                    println!("Spill any reg {any_reg}");
                }
                self.spilled += BlockReg::Any(any_reg);
                self.gen_pre_handle_spilled_inst(any_reg, fixed_reg, TransferOp::Write, opcodes);
            }
        }
    }

    fn get_output_reg(&mut self, any_reg: u16, next_live_range: &BlockRegSet, used_regs: &BlockRegSet, opcodes: &mut Vec<u32>) -> Reg {
        match self.get_stored_mapping(any_reg) {
            Reg::None => {
                self.spilled -= BlockReg::Any(any_reg);
                self.allocate_reg(any_reg, next_live_range, used_regs, opcodes)
            }
            stored_mapping => *stored_mapping,
        }
    }

    fn relocate_guest_regs(&mut self, guest_regs: RegReserve, next_live_range: &BlockRegSet, inputs: &BlockRegSet, is_input: bool, opcodes: &mut Vec<u32>) {
        let mut relocatable_regs = RegReserve::new();
        for guest_reg in guest_regs {
            if *self.get_stored_mapping(guest_reg as u16) != guest_reg
                // Check if reg is used as a fixed input for something else
                && !next_live_range.contains(BlockReg::Fixed(guest_reg))
            {
                relocatable_regs += guest_reg;
            }
        }

        if relocatable_regs.is_empty() {
            return;
        }

        let mut new_lru = FixedFifo::new();
        while !self.lru_reg.is_empty() {
            let reg = *self.lru_reg.front();
            self.lru_reg.pop_front();
            if !relocatable_regs.is_reserved(reg) {
                new_lru.push_back(reg);
            }
        }
        for guest_reg in relocatable_regs {
            new_lru.push_back(guest_reg);
        }
        self.lru_reg = new_lru;

        for guest_reg in relocatable_regs {
            if let Some(currently_used_by) = *self.get_stored_mapping_reverse(guest_reg) {
                if DEBUG && unsafe { BLOCK_LOG } {
                    println!("relocate guest spill {currently_used_by} for {guest_reg:?}");
                }
                if inputs.contains(BlockReg::Any(currently_used_by)) || next_live_range.contains(BlockReg::Any(currently_used_by)) {
                    self.spilled += BlockReg::Any(currently_used_by);
                    self.gen_pre_handle_spilled_inst(currently_used_by, guest_reg, TransferOp::Write, opcodes);
                }
                self.remove_stored_mapping(currently_used_by);
            }
        }

        for guest_reg in relocatable_regs {
            let stored_mapping = *self.get_stored_mapping(guest_reg as u16);
            if stored_mapping != Reg::None {
                if is_input {
                    self.gen_pre_move_reg(guest_reg, stored_mapping, opcodes);
                }
                self.remove_stored_mapping(guest_reg as u16);
                self.set_stored_mapping(guest_reg as u16, guest_reg);
                relocatable_regs -= guest_reg;
            }
        }

        for guest_reg in relocatable_regs {
            if is_input {
                debug_assert!(self.spilled.contains(BlockReg::Any(guest_reg as u16)), "{guest_reg:?}, {relocatable_regs:?}");
                self.spilled -= BlockReg::Any(guest_reg as u16);
                self.gen_pre_handle_spilled_inst(guest_reg as u16, guest_reg, TransferOp::Read, opcodes);
            }
            self.set_stored_mapping(guest_reg as u16, guest_reg);
        }
    }

    pub fn inst_allocate(&mut self, inst: &BlockInst, next_live_range: &BlockRegSet, opcodes: &mut Vec<u32>) {
        if DEBUG && unsafe { BLOCK_LOG } {
            println!("allocate reg for {inst:?}");
        }

        let (inputs, outputs) = inst.get_io();
        if unlikely(inputs.is_empty() && outputs.is_empty()) {
            return;
        }

        let inputs = *inputs;
        let mut outputs = *outputs;
        // cpsr changes don't need reg allocations, since they write to the actual cpsr reg
        // it's only part of output to keep track for savereg
        outputs -= BlockReg::from(Reg::CPSR);
        let used_regs = inputs + outputs;

        if DEBUG && unsafe { BLOCK_LOG } {
            println!("inputs: {inputs:?}, outputs: {outputs:?}");
            println!("used regs {:?}", used_regs);
        }

        self.relocate_guest_regs(inputs.get_guests().get_gp_regs(), next_live_range, &inputs, true, opcodes);
        self.relocate_guest_regs(outputs.get_guests().get_gp_regs(), next_live_range, &inputs, false, opcodes);

        if DEBUG && unsafe { BLOCK_LOG } {
            println!("pre mapping {:?}", self.stored_mapping_reverse);
            println!("pre spilled {:?}", self.spilled);
        }

        for any_input_reg in inputs.iter_any() {
            let reg = self.get_input_reg(any_input_reg, next_live_range, &used_regs, opcodes);
            self.stored_mapping_inputs[any_input_reg as usize] = reg;
        }

        for fixed_reg_output in outputs.get_fixed().get_gp_lr_regs() {
            self.remove_fixed_reg(fixed_reg_output, next_live_range, opcodes);
            self.dirty_regs += fixed_reg_output;
        }

        for any_output_reg in outputs.iter_any() {
            let reg = self.get_output_reg(any_output_reg, next_live_range, &used_regs, opcodes);
            self.dirty_regs += reg;
        }

        self.gen_flush(opcodes);

        if DEBUG && unsafe { BLOCK_LOG } {
            println!("after mapping {:?}", self.stored_mapping_reverse);
            println!("after spilled {:?}", self.spilled);
        }

        if DEBUG {
            for (any_reg, &stored_mapping) in self.stored_mapping.iter().enumerate() {
                if stored_mapping != Reg::None {
                    assert_eq!(*self.get_stored_mapping_reverse(stored_mapping), Some(any_reg as u16));
                }
            }
            for (reg, &mapped_reg) in self.stored_mapping_reverse.iter().enumerate() {
                if let Some(mapped_reg) = mapped_reg {
                    assert_eq!(Reg::from(reg as u8), *self.get_stored_mapping(mapped_reg));
                }
            }
        }
    }

    pub fn ensure_global_mappings(&mut self, output_regs: BlockRegSet, opcodes: &mut Vec<u32>) {
        for output_reg in output_regs.iter_any() {
            match self.get_global_mapping(output_reg) {
                Reg::None => {
                    let stored_mapping = *self.get_stored_mapping(output_reg);
                    if stored_mapping != Reg::None {
                        self.remove_stored_mapping(output_reg);
                        self.spilled += BlockReg::Any(output_reg);
                        self.gen_pre_handle_spilled_inst(output_reg, stored_mapping, TransferOp::Write, opcodes);
                    }
                }
                desired_reg_mapping => {
                    let stored_mapping = *self.get_stored_mapping(output_reg);
                    if desired_reg_mapping == stored_mapping {
                        // Already at correct register, skip
                        continue;
                    }

                    if let Some(currently_used_by) = *self.get_stored_mapping_reverse(desired_reg_mapping) {
                        // Some other any reg is using the desired reg
                        if output_regs.contains(BlockReg::Any(currently_used_by)) {
                            // other any reg is part of required output
                            match self.get_global_mapping(currently_used_by) {
                                Reg::None => {
                                    // other any reg is part of predetermined spilled
                                    self.remove_stored_mapping(currently_used_by);
                                    self.spilled += BlockReg::Any(currently_used_by);
                                    self.gen_pre_handle_spilled_inst(currently_used_by, desired_reg_mapping, TransferOp::Write, opcodes);
                                }
                                _ => {
                                    let mut moved = false;
                                    // find a mapped any reg that is not part of output for back up
                                    for (i, unused_reg_mapped) in self.stored_mapping_reverse.iter().enumerate() {
                                        if let Some(unused_reg_mapped) = unused_reg_mapped {
                                            if !output_regs.contains(BlockReg::Any(*unused_reg_mapped)) {
                                                let stored_mapping = Reg::from(i as u8);
                                                self.remove_stored_mapping(*unused_reg_mapped);
                                                self.set_stored_mapping(currently_used_by, stored_mapping);
                                                self.gen_pre_move_reg(stored_mapping, desired_reg_mapping, opcodes);
                                                moved = true;
                                                break;
                                            }
                                        }
                                    }

                                    if !moved {
                                        // no unused any reg found, just spill the any reg using the desired reg
                                        self.remove_stored_mapping(currently_used_by);
                                        self.spilled += BlockReg::Any(currently_used_by);
                                        self.gen_pre_handle_spilled_inst(currently_used_by, desired_reg_mapping, TransferOp::Write, opcodes);
                                    }
                                }
                            }
                        } else {
                            self.remove_stored_mapping(currently_used_by);
                        }
                    }

                    if stored_mapping != Reg::None {
                        self.remove_stored_mapping(output_reg);
                        self.gen_pre_move_reg(desired_reg_mapping, stored_mapping, opcodes);
                    } else if self.spilled.contains(BlockReg::Any(output_reg)) {
                        self.spilled -= BlockReg::Any(output_reg);
                        self.gen_pre_handle_spilled_inst(output_reg, desired_reg_mapping, TransferOp::Read, opcodes);
                    } else {
                        panic!("required output reg {output_reg:?} must already have a value");
                    }
                    self.set_stored_mapping(output_reg, desired_reg_mapping);
                }
            }
        }
        self.gen_flush(opcodes);
    }
}

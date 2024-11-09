use crate::bitset::Bitset;
use crate::jit::assembler::block_asm::BLOCK_LOG;
use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockInst, BlockInstKind, BlockTransferOp};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::MemoryAmount;
use crate::utils::{HeapMem, NoHashMap};
use std::hint::unreachable_unchecked;

const DEBUG: bool = true;

pub const ALLOCATION_REGS: RegReserve = reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11);
const SCRATCH_REGS: RegReserve = reg_reserve!(Reg::R0, Reg::R1, Reg::R2, Reg::R3, Reg::R12, Reg::LR);

pub struct BlockRegAllocator {
    pub global_mapping: NoHashMap<u16, Reg>,
    stored_mapping: HeapMem<Reg, { ANY_REG_LIMIT as usize }>, // mappings to real registers
    stored_mapping_reverse: [Option<u16>; Reg::PC as usize],
    spilled: BlockRegSet,
    pub dirty_regs: RegReserve,
    pub pre_allocate_insts: Vec<BlockInst>,
}

impl BlockRegAllocator {
    pub fn new() -> Self {
        BlockRegAllocator {
            global_mapping: NoHashMap::default(),
            stored_mapping: HeapMem::new(),
            stored_mapping_reverse: [None; Reg::PC as usize],
            spilled: BlockRegSet::new(),
            dirty_regs: RegReserve::new(),
            pre_allocate_insts: Vec::new(),
        }
    }

    pub fn init_inputs(&mut self, input_regs: BlockRegSet) {
        self.stored_mapping.fill(Reg::None);
        self.stored_mapping_reverse.fill(None);
        self.spilled.clear();
        for any_input_reg in input_regs.iter_any() {
            if let Some(&global_mapping) = self.global_mapping.get(&any_input_reg) {
                match global_mapping {
                    Reg::None => self.spilled += BlockReg::Any(any_input_reg),
                    _ => self.set_stored_mapping(any_input_reg, global_mapping),
                }
            }
        }
    }

    fn gen_pre_handle_spilled_inst(&mut self, any_reg: u16, mapping: Reg, op: BlockTransferOp) {
        self.dirty_regs += mapping;
        self.pre_allocate_insts.push(
            BlockInstKind::Transfer {
                op,
                operands: [BlockReg::Fixed(mapping).into(), BlockReg::Fixed(Reg::SP).into(), (any_reg as u32 * 4).into()],
                signed: false,
                amount: MemoryAmount::Word,
                add_to_base: true,
            }
            .into(),
        );
    }

    fn gen_pre_move_reg(&mut self, dst: Reg, src: Reg) {
        self.dirty_regs += dst;
        self.pre_allocate_insts.push(
            BlockInstKind::Alu2Op0 {
                op: BlockAluOp::Mov,
                operands: [BlockReg::Fixed(dst).into(), BlockReg::Fixed(src).into()],
                set_cond: BlockAluSetCond::None,
                thumb_pc_aligned: false,
            }
            .into(),
        );
    }

    fn set_stored_mapping(&mut self, any_reg: u16, reg: Reg) {
        self.stored_mapping[any_reg as usize] = reg;
        self.stored_mapping_reverse[reg as usize] = Some(any_reg);
    }

    fn remove_stored_mapping(&mut self, any_reg: u16) {
        let stored_mapping = self.stored_mapping[any_reg as usize];
        self.stored_mapping[any_reg as usize] = Reg::None;
        self.stored_mapping_reverse[stored_mapping as usize] = None;
    }

    fn swap_stored_mapping(&mut self, any_reg: u16, other_any_reg: u16) {
        let stored_mapping = self.stored_mapping[any_reg as usize];
        let stored_mapping_other = self.stored_mapping[other_any_reg as usize];
        self.stored_mapping.swap(any_reg as usize, other_any_reg as usize);
        if stored_mapping != Reg::None {
            self.stored_mapping_reverse[stored_mapping as usize] = Some(other_any_reg);
        }
        if stored_mapping_other != Reg::None {
            self.stored_mapping_reverse[stored_mapping_other as usize] = Some(any_reg);
        }
    }

    fn allocate_common(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &BlockRegSet) -> Option<Reg> {
        for reg in ALLOCATION_REGS {
            if self.stored_mapping_reverse[reg as usize].is_none() && !used_regs.contains(BlockReg::Fixed(reg)) && !live_ranges[1].contains(BlockReg::Fixed(reg)) {
                self.set_stored_mapping(any_reg, reg);
                return Some(reg);
            }
        }

        for (i, used_any_reg) in self.stored_mapping_reverse.iter().enumerate() {
            let reg = Reg::from(i as u8);
            if ALLOCATION_REGS.is_reserved(reg) {
                if let Some(used_any_reg) = *used_any_reg {
                    if !used_regs.contains(BlockReg::Any(used_any_reg)) && !live_ranges[1].contains(BlockReg::Any(used_any_reg)) && !live_ranges[1].contains(BlockReg::Fixed(reg)) {
                        self.swap_stored_mapping(any_reg, used_any_reg);
                        return Some(reg);
                    }
                }
            }
        }

        None
    }

    fn allocate_and_spill(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &BlockRegSet, allowed_regs: RegReserve) -> Option<Reg> {
        for (i, mapped_reg) in self.stored_mapping_reverse.iter().enumerate() {
            let reg = Reg::from(i as u8);

            if mapped_reg.is_none() && allowed_regs.is_reserved(reg) && !live_ranges[1].contains(BlockReg::Fixed(reg)) && !used_regs.contains(BlockReg::Fixed(reg)) {
                self.set_stored_mapping(any_reg, reg);
                return Some(reg);
            }
        }

        for (i, mapped_reg) in self.stored_mapping_reverse.iter().enumerate() {
            let reg = Reg::from(i as u8);

            if let &Some(mapped_reg) = mapped_reg {
                if allowed_regs.is_reserved(reg) && !used_regs.contains(BlockReg::Any(mapped_reg)) && !live_ranges[1].contains(BlockReg::Any(mapped_reg)) {
                    self.swap_stored_mapping(any_reg, mapped_reg);
                    return Some(reg);
                }
            }
        }

        for (i, mapped_reg) in self.stored_mapping_reverse.iter().enumerate() {
            let reg = Reg::from(i as u8);

            if let &Some(mapped_reg) = mapped_reg {
                if allowed_regs.is_reserved(reg) && !used_regs.contains(BlockReg::Any(mapped_reg)) {
                    self.spilled += BlockReg::Any(mapped_reg);
                    self.gen_pre_handle_spilled_inst(mapped_reg, reg, BlockTransferOp::Write);
                    self.swap_stored_mapping(any_reg, mapped_reg);
                    return Some(reg);
                }
            }
        }

        None
    }

    fn allocate_local(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &BlockRegSet) -> Reg {
        for reg in SCRATCH_REGS {
            if self.stored_mapping_reverse[reg as usize].is_none() && !live_ranges[1].contains(BlockReg::Fixed(reg)) {
                self.set_stored_mapping(any_reg, reg);
                return reg;
            }
        }

        if let Some(reg) = self.allocate_common(any_reg, live_ranges, used_regs) {
            return reg;
        }

        if let Some(reg) = self.allocate_and_spill(any_reg, live_ranges, used_regs, SCRATCH_REGS + ALLOCATION_REGS) {
            return reg;
        }

        unsafe { unreachable_unchecked() }
    }

    fn allocate_reg(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &BlockRegSet) -> Reg {
        if let Some(reg) = self.allocate_common(any_reg, live_ranges, used_regs) {
            return reg;
        }

        if let Some(reg) = self.allocate_and_spill(any_reg, live_ranges, used_regs, ALLOCATION_REGS) {
            return reg;
        }

        if let Some(reg) = self.allocate_and_spill(any_reg, live_ranges, used_regs, SCRATCH_REGS) {
            return reg;
        }

        unsafe { unreachable_unchecked() }
    }

    fn get_input_reg(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &BlockRegSet) -> Reg {
        match self.stored_mapping[any_reg as usize] {
            Reg::None => {
                if self.spilled.contains(BlockReg::Any(any_reg)) {
                    let reg = if live_ranges.last().unwrap().contains(BlockReg::Any(any_reg)) {
                        self.allocate_reg(any_reg, live_ranges, used_regs)
                    } else {
                        self.allocate_local(any_reg, live_ranges, used_regs)
                    };
                    self.spilled -= BlockReg::Any(any_reg);
                    self.gen_pre_handle_spilled_inst(any_reg, reg, BlockTransferOp::Read);
                    reg
                } else {
                    panic!("input reg {any_reg} must be allocated")
                }
            }
            stored_mapping => stored_mapping,
        }
    }

    fn remove_fixed_reg(&mut self, fixed_reg: Reg, live_ranges: &[BlockRegSet]) {
        if DEBUG && unsafe { BLOCK_LOG } {
            println!("Remove fixed reg {fixed_reg:?}");
        }
        if let Some(any_reg) = self.stored_mapping_reverse[fixed_reg as usize] {
            self.remove_stored_mapping(any_reg);
            if DEBUG && unsafe { BLOCK_LOG } {
                println!("Remove stored mapping {any_reg}");
            }
            if live_ranges[1].contains(BlockReg::Any(any_reg)) {
                if DEBUG && unsafe { BLOCK_LOG } {
                    println!("Spill any reg {any_reg}");
                }
                self.spilled += BlockReg::Any(any_reg);
                self.gen_pre_handle_spilled_inst(any_reg, fixed_reg, BlockTransferOp::Write);
            }
        }
    }

    fn get_output_reg(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &BlockRegSet) -> Reg {
        match self.stored_mapping[any_reg as usize] {
            Reg::None => {
                self.spilled -= BlockReg::Any(any_reg);
                if live_ranges.last().unwrap().contains(BlockReg::Any(any_reg)) {
                    self.allocate_reg(any_reg, live_ranges, used_regs)
                } else {
                    self.allocate_local(any_reg, live_ranges, used_regs)
                }
            }
            stored_mapping => stored_mapping,
        }
    }

    fn relocate_guest_regs(&mut self, guest_regs: RegReserve, live_ranges: &[BlockRegSet], inputs: &BlockRegSet, used_regs: &BlockRegSet, is_input: bool) {
        let mut relocatable_regs = RegReserve::new();
        for guest_reg in guest_regs {
            if self.stored_mapping[guest_reg as usize] != guest_reg
                // Check if reg is used as a fixed input for something else
                && !live_ranges[1].contains(BlockReg::Fixed(guest_reg))
            {
                relocatable_regs += guest_reg;
            }
        }

        let mut spilled_regs = Vec::new();
        for guest_reg in relocatable_regs {
            if let Some(currently_used_by) = self.stored_mapping_reverse[guest_reg as usize] {
                if DEBUG && unsafe { BLOCK_LOG } {
                    println!("relocate guest spill {currently_used_by} for {guest_reg:?}");
                }
                if inputs.contains(BlockReg::Any(currently_used_by)) || live_ranges[1].contains(BlockReg::Any(currently_used_by)) {
                    self.spilled += BlockReg::Any(currently_used_by);
                    self.gen_pre_handle_spilled_inst(currently_used_by, guest_reg, BlockTransferOp::Write);
                    spilled_regs.push((BlockReg::Any(currently_used_by), guest_reg, self.pre_allocate_insts.len() - 1));
                }
                self.remove_stored_mapping(currently_used_by);
            }
        }

        let mut dirty_regs = RegReserve::new();
        for guest_reg in relocatable_regs {
            let reg_mapped = self.stored_mapping[guest_reg as usize];
            if reg_mapped != Reg::None {
                if is_input {
                    self.gen_pre_move_reg(guest_reg, reg_mapped);
                    dirty_regs += guest_reg;
                }
                self.remove_stored_mapping(guest_reg as u16);
                self.set_stored_mapping(guest_reg as u16, guest_reg);
                relocatable_regs -= guest_reg;
            }
        }

        for guest_reg in relocatable_regs {
            if is_input {
                debug_assert!(self.spilled.contains(BlockReg::Any(guest_reg as u16)));
                self.spilled -= BlockReg::Any(guest_reg as u16);
                self.gen_pre_handle_spilled_inst(guest_reg as u16, guest_reg, BlockTransferOp::Read);
                dirty_regs += guest_reg;
            }
            self.set_stored_mapping(guest_reg as u16, guest_reg);
        }

        let mut new_pre_allocate_insts_filter = Bitset::<1>::new();
        for (spilled_reg, previous_mapping, pre_allocate_index) in spilled_regs {
            if inputs.contains(spilled_reg) && !dirty_regs.is_reserved(previous_mapping) {
                let reg = if live_ranges.last().unwrap().contains(spilled_reg) {
                    self.allocate_reg(spilled_reg.as_any(), live_ranges, used_regs)
                } else {
                    self.allocate_local(spilled_reg.as_any(), live_ranges, used_regs)
                };
                self.spilled -= spilled_reg;
                self.gen_pre_move_reg(reg, previous_mapping);
                new_pre_allocate_insts_filter += pre_allocate_index;
            }
        }

        if !new_pre_allocate_insts_filter.is_empty() {
            let mut new_pre_allocate_insts = Vec::new();
            for (i, inst) in self.pre_allocate_insts.iter().enumerate() {
                if !new_pre_allocate_insts_filter.contains(i) {
                    new_pre_allocate_insts.push(inst.clone());
                }
            }
            self.pre_allocate_insts = new_pre_allocate_insts;
        }
    }

    pub fn inst_allocate(&mut self, inst: &mut BlockInst, live_ranges: &[BlockRegSet]) {
        self.pre_allocate_insts.clear();

        if DEBUG && unsafe { BLOCK_LOG } {
            println!("allocate reg for {inst:?}");
        }

        let (inputs, outputs) = inst.get_io();
        if inputs.is_empty() && outputs.is_empty() {
            return;
        }

        let used_regs = inputs + outputs;

        if DEBUG && unsafe { BLOCK_LOG } {
            println!("inputs: {inputs:?}, outputs: {outputs:?}");
            println!("used regs {:?}", used_regs);
        }

        self.relocate_guest_regs(inputs.get_guests().get_gp_regs(), live_ranges, &inputs, &used_regs, true);
        self.relocate_guest_regs(outputs.get_guests().get_gp_regs(), live_ranges, &inputs, &used_regs, false);

        if DEBUG && unsafe { BLOCK_LOG } {
            println!("pre mapping {:?}", self.stored_mapping_reverse);
            println!("pre spilled {:?}", self.spilled);
        }

        for any_input_reg in inputs.iter_any() {
            let reg = self.get_input_reg(any_input_reg, live_ranges, &used_regs);
            inst.replace_input_regs(BlockReg::Any(any_input_reg), BlockReg::Fixed(reg));
        }

        for fixed_reg_output in outputs.get_fixed().get_gp_lr_regs() {
            self.remove_fixed_reg(fixed_reg_output, live_ranges);
            self.dirty_regs += fixed_reg_output;
        }

        for any_output_reg in outputs.iter_any() {
            let reg = self.get_output_reg(any_output_reg, live_ranges, &used_regs);
            inst.replace_output_regs(BlockReg::Any(any_output_reg), BlockReg::Fixed(reg));
            self.dirty_regs += reg;
        }

        if DEBUG && unsafe { BLOCK_LOG } {
            println!("after mapping {:?}", self.stored_mapping_reverse);
            println!("after spilled {:?}", self.spilled);
        }
    }

    pub fn ensure_global_mappings(&mut self, output_regs: BlockRegSet) {
        self.pre_allocate_insts.clear();

        for output_reg in output_regs.iter_any() {
            match *self.global_mapping.get(&output_reg).unwrap() {
                Reg::None => {
                    let stored_mapping = self.stored_mapping[output_reg as usize];
                    if stored_mapping != Reg::None {
                        self.remove_stored_mapping(output_reg);
                        self.spilled += BlockReg::Any(output_reg);
                        self.gen_pre_handle_spilled_inst(output_reg, stored_mapping, BlockTransferOp::Write);
                    }
                }
                desired_reg_mapping => {
                    let stored_mapping = self.stored_mapping[output_reg as usize];
                    if desired_reg_mapping == stored_mapping {
                        // Already at correct register, skip
                        continue;
                    }

                    if let Some(currently_used_by) = self.stored_mapping_reverse[desired_reg_mapping as usize] {
                        // Some other any reg is using the desired reg
                        if output_regs.contains(BlockReg::Any(currently_used_by)) {
                            // other any reg is part of required output
                            match self.global_mapping.get(&currently_used_by).unwrap() {
                                Reg::None => {
                                    // other any reg is part of predetermined spilled
                                    self.remove_stored_mapping(currently_used_by);
                                    self.spilled += BlockReg::Any(currently_used_by);
                                    self.gen_pre_handle_spilled_inst(currently_used_by, desired_reg_mapping, BlockTransferOp::Write);
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
                                                self.gen_pre_move_reg(stored_mapping, desired_reg_mapping);
                                                moved = true;
                                                break;
                                            }
                                        }
                                    }

                                    if !moved {
                                        // no unused any reg found, just spill the any reg using the desired reg
                                        self.remove_stored_mapping(currently_used_by);
                                        self.spilled += BlockReg::Any(currently_used_by);
                                        self.gen_pre_handle_spilled_inst(currently_used_by, desired_reg_mapping, BlockTransferOp::Write);
                                    }
                                }
                            }
                        } else {
                            self.remove_stored_mapping(currently_used_by);
                        }
                    }

                    if stored_mapping != Reg::None {
                        self.remove_stored_mapping(output_reg);
                        self.gen_pre_move_reg(desired_reg_mapping, stored_mapping);
                    } else if self.spilled.contains(BlockReg::Any(output_reg)) {
                        self.spilled -= BlockReg::Any(output_reg);
                        self.gen_pre_handle_spilled_inst(output_reg, desired_reg_mapping, BlockTransferOp::Read);
                    } else {
                        panic!("required output reg {output_reg:?} must already have a value");
                    }
                    self.set_stored_mapping(output_reg, desired_reg_mapping);
                }
            }
        }
    }
}

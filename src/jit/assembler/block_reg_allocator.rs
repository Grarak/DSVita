use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockInst, BlockTransferOp};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::MemoryAmount;
use crate::utils::{HeapMem, NoHashMap, NoHashSet};

pub const ALLOCATION_REGS: RegReserve = reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11);
const SCRATCH_REGS: RegReserve = reg_reserve!(Reg::R0, Reg::R1, Reg::R2, Reg::R3, Reg::R12);

pub struct BlockRegAllocator {
    global_mapping: NoHashMap<u16, Reg>,
    stored_mapping: HeapMem<Reg, { ANY_REG_LIMIT as usize }>, // mappings to real registers
    stored_mapping_reverse: [Option<u16>; Reg::SP as usize],
    spilled: NoHashSet<u16>, // regs that are spilled
    pub pre_allocate_insts: Vec<BlockInst>,
}

impl BlockRegAllocator {
    pub fn new(global_mapping: NoHashMap<u16, Reg>) -> Self {
        BlockRegAllocator {
            global_mapping,
            stored_mapping: HeapMem::new(),
            stored_mapping_reverse: [None; Reg::SP as usize],
            spilled: NoHashSet::default(),
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
                    Reg::None => {
                        self.spilled.insert(any_input_reg);
                    }
                    _ => self.set_stored_mapping(any_input_reg, global_mapping),
                }
            }
        }
    }

    fn gen_pre_handle_spilled_inst(&mut self, any_reg: u16, mapping: Reg, op: BlockTransferOp) {
        self.pre_allocate_insts.push(BlockInst::Transfer {
            op,
            operands: [BlockReg::Fixed(mapping).into(), BlockReg::Fixed(Reg::SP).into(), (any_reg as u32 * 4).into()],
            signed: false,
            amount: MemoryAmount::Word,
            add_to_base: true,
        });
    }

    fn gen_pre_move_reg(&mut self, dst: Reg, src: Reg) {
        self.pre_allocate_insts.push(BlockInst::Alu2Op0 {
            op: BlockAluOp::Mov,
            operands: [BlockReg::Fixed(dst).into(), BlockReg::Fixed(src).into()],
            set_cond: BlockAluSetCond::None,
            thumb_pc_aligned: false,
        });
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

    fn allocate_common(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) -> Option<Reg> {
        for reg in ALLOCATION_REGS {
            if self.stored_mapping_reverse[reg as usize].is_none() {
                self.set_stored_mapping(any_reg, reg);
                return Some(reg);
            }
        }

        for (i, used_any_reg) in self.stored_mapping_reverse.iter().enumerate() {
            let reg = Reg::from(i as u8);
            if ALLOCATION_REGS.is_reserved(reg) {
                if let Some(used_any_reg) = *used_any_reg {
                    if !used_regs[0].contains(BlockReg::Any(used_any_reg)) && !live_ranges[1].contains(BlockReg::Any(used_any_reg)) && !live_ranges[1].contains(BlockReg::Fixed(reg)) {
                        self.swap_stored_mapping(any_reg, used_any_reg);
                        return Some(reg);
                    }
                }
            }
        }

        None
    }

    fn allocate_and_spill(&mut self, any_reg: u16, used_regs: &[BlockRegSet], allowed_regs: RegReserve) -> Option<Reg> {
        let mut greatest_distance = 0;
        let mut greatest_distance_reg = 0;
        for (i, mapped_reg) in self.stored_mapping_reverse.iter().enumerate() {
            if let &Some(mapped_reg) = mapped_reg {
                let reg = Reg::from(i as u8);
                if allowed_regs.is_reserved(reg) && !used_regs[0].contains(BlockReg::Any(mapped_reg)) {
                    for i in 1..used_regs.len() {
                        if used_regs[i].contains(BlockReg::Any(mapped_reg)) || used_regs[i].contains(BlockReg::Fixed(reg)) {
                            if i > greatest_distance {
                                greatest_distance = i;
                                greatest_distance_reg = mapped_reg;
                            }
                            break;
                        }
                    }
                }
            }
        }

        if greatest_distance != 0 {
            let reg = self.stored_mapping[greatest_distance_reg as usize];
            self.spilled.insert(greatest_distance_reg);
            self.gen_pre_handle_spilled_inst(greatest_distance_reg, reg, BlockTransferOp::Write);
            self.swap_stored_mapping(any_reg, greatest_distance_reg);
            return Some(reg);
        }

        None
    }

    fn allocate_local(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) -> Reg {
        let mut live_ranges_until_expiration = BlockRegSet::new();
        let mut used_fixed_until_expiration = used_regs[0].get_fixed();
        for i in 1..live_ranges.len() {
            if !live_ranges[i].contains(BlockReg::Any(any_reg)) {
                break;
            }
            live_ranges_until_expiration += live_ranges[i];
            used_fixed_until_expiration += used_regs[i].get_fixed();
        }

        for reg in SCRATCH_REGS {
            if !used_fixed_until_expiration.is_reserved(reg) && self.stored_mapping_reverse[reg as usize].is_none() && !live_ranges[1].contains(BlockReg::Fixed(reg)) {
                self.set_stored_mapping(any_reg, reg);
                return reg;
            }
        }

        if let Some(reg) = self.allocate_common(any_reg, live_ranges, used_regs) {
            return reg;
        }

        for (i, mapped_reg) in self.stored_mapping_reverse.iter().enumerate() {
            if let &Some(mapped_reg) = mapped_reg {
                let reg = Reg::from(i as u8);
                if !used_regs[0].contains(BlockReg::Any(mapped_reg))
                    && !live_ranges_until_expiration.contains(BlockReg::Any(mapped_reg))
                    && !live_ranges_until_expiration.contains(BlockReg::Fixed(reg))
                {
                    self.spilled.insert(mapped_reg);
                    self.gen_pre_handle_spilled_inst(mapped_reg, reg, BlockTransferOp::Write);
                    self.swap_stored_mapping(any_reg, mapped_reg);
                    return reg;
                }
            }
        }

        if let Some(reg) = self.allocate_and_spill(any_reg, used_regs, SCRATCH_REGS + ALLOCATION_REGS) {
            return reg;
        }

        todo!()
    }

    fn allocate_reg(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) -> Reg {
        if let Some(reg) = self.allocate_common(any_reg, live_ranges, used_regs) {
            return reg;
        }

        if let Some(reg) = self.allocate_and_spill(any_reg, used_regs, ALLOCATION_REGS) {
            return reg;
        }

        todo!()
    }

    fn get_input_reg(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) -> Reg {
        match self.stored_mapping[any_reg as usize] {
            Reg::None => {
                if self.spilled.contains(&any_reg) {
                    let reg = if live_ranges.last().unwrap().contains(BlockReg::Any(any_reg)) {
                        self.allocate_reg(any_reg, live_ranges, used_regs)
                    } else {
                        self.allocate_local(any_reg, live_ranges, used_regs)
                    };
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
        if let Some(any_reg) = self.stored_mapping_reverse[fixed_reg as usize] {
            self.remove_stored_mapping(any_reg);
            if live_ranges[1].contains(BlockReg::Any(any_reg)) {
                self.spilled.insert(any_reg);
                self.gen_pre_handle_spilled_inst(any_reg, fixed_reg, BlockTransferOp::Write);
            }
        }
    }

    fn get_output_reg(&mut self, any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) -> Reg {
        match self.stored_mapping[any_reg as usize] {
            Reg::None => {
                if live_ranges.last().unwrap().contains(BlockReg::Any(any_reg)) {
                    self.allocate_reg(any_reg, live_ranges, used_regs)
                } else {
                    self.allocate_local(any_reg, live_ranges, used_regs)
                }
            }
            stored_mapping => stored_mapping,
        }
    }

    pub fn inst_allocate(&mut self, inst: &mut BlockInst, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) {
        self.pre_allocate_insts.clear();

        let (inputs, outputs) = inst.get_io();
        if inputs.is_empty() && outputs.is_empty() {
            return;
        }

        for any_input_reg in inputs.iter_any() {
            let reg = self.get_input_reg(any_input_reg, live_ranges, used_regs);
            inst.replace_input_regs(BlockReg::Any(any_input_reg), BlockReg::Fixed(reg));
        }

        for fixed_reg_output in outputs.get_fixed().get_gp_regs() {
            self.remove_fixed_reg(fixed_reg_output, live_ranges);
        }

        for any_output_reg in outputs.iter_any() {
            let reg = self.get_output_reg(any_output_reg, live_ranges, used_regs);
            inst.replace_output_regs(BlockReg::Any(any_output_reg), BlockReg::Fixed(reg));
        }
    }

    pub fn ensure_global_mappings(&mut self, output_regs: BlockRegSet) {
        self.pre_allocate_insts.clear();

        for output_reg in output_regs.iter_any() {
            match self.global_mapping.get(&output_reg).unwrap() {
                Reg::None => {
                    let stored_mapping = self.stored_mapping[output_reg as usize];
                    if stored_mapping != Reg::None {
                        self.remove_stored_mapping(output_reg);
                        self.spilled.insert(output_reg);
                        self.gen_pre_handle_spilled_inst(output_reg, stored_mapping, BlockTransferOp::Write);
                    }
                }
                desired_reg_mapping => {
                    let desired_reg_mapping = *desired_reg_mapping;

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
                                    self.spilled.insert(currently_used_by);
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
                                        self.spilled.insert(currently_used_by);
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
                    } else if self.spilled.contains(&output_reg) {
                        self.spilled.remove(&output_reg);
                        self.gen_pre_handle_spilled_inst(output_reg, desired_reg_mapping, BlockTransferOp::Read);
                    } else {
                        panic!("required output reg must already have a value");
                    }
                    self.set_stored_mapping(output_reg, desired_reg_mapping);
                }
            }
        }
    }
}

use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockInst, BlockTransferOp};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::MemoryAmount;
use crate::utils::{HeapMem, NoHashMap, NoHashSet};

const ALLOCATION_REGS: RegReserve = reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11);
const SCRATCH_REGS: RegReserve = reg_reserve!(Reg::R0, Reg::R1, Reg::R2, Reg::R3, Reg::R12);

pub struct BlockRegAllocator {
    global_regs_mapping: NoHashMap<u16, Reg>,
    allocated_real_regs: RegReserve,
    stored_mapping: HeapMem<Reg, { ANY_REG_LIMIT as usize }>, // mappings to real registers
    stored_mapping_reverse: [Option<u16>; Reg::SP as usize],
    spilled: NoHashSet<u16>, // regs that are spilled
    pub pre_allocate_insts: Vec<BlockInst>,
}

impl BlockRegAllocator {
    pub fn new(global_regs: BlockRegSet) -> Self {
        let mut global_regs_allocated_stored = RegReserve::new();
        let mut global_regs_mapping = NoHashMap::default();
        let mut stored_mapping = HeapMem::new();
        let mut stored_mapping_reverse = [None; Reg::SP as usize];

        'outer: for any_reg in global_regs.iter_any() {
            if global_regs_allocated_stored.len() != (ALLOCATION_REGS + SCRATCH_REGS).len() {
                for reg in ALLOCATION_REGS {
                    if !global_regs_allocated_stored.is_reserved(reg) {
                        global_regs_allocated_stored += reg;
                        global_regs_mapping.insert(any_reg, reg);
                        stored_mapping[any_reg as usize] = reg;
                        stored_mapping_reverse[reg as usize] = Some(any_reg);
                        continue 'outer;
                    }
                }
                for reg in SCRATCH_REGS {
                    if !global_regs_allocated_stored.is_reserved(reg) {
                        global_regs_allocated_stored += reg;
                        global_regs_mapping.insert(any_reg, reg);
                        stored_mapping[any_reg as usize] = reg;
                        stored_mapping_reverse[reg as usize] = Some(any_reg);
                        continue 'outer;
                    }
                }
            } else {
                global_regs_mapping.insert(any_reg, Reg::None);
            }
        }

        BlockRegAllocator {
            global_regs_mapping,
            allocated_real_regs: global_regs_allocated_stored,
            stored_mapping,
            stored_mapping_reverse,
            spilled: NoHashSet::default(),
            pre_allocate_insts: Vec::new(),
        }
    }

    fn get_expiration_info(any_reg: u16, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) -> (bool, RegReserve) {
        let mut fixed_regs_used = used_regs.first().unwrap().get_fixed();
        for i in 1..live_ranges.len() {
            if !live_ranges[i].contains(BlockReg::Any(any_reg)) {
                return (true, fixed_regs_used);
            }
            fixed_regs_used += used_regs[i].get_fixed();
        }
        (false, fixed_regs_used)
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

    fn allocate_reg(&mut self, any_reg: u16, current_outputs: BlockRegSet, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) -> Reg {
        let (will_expire, fixed_regs_used) = Self::get_expiration_info(any_reg, live_ranges, used_regs);

        if will_expire {
            for scratch_reg in SCRATCH_REGS {
                if !fixed_regs_used.is_reserved(scratch_reg) && !self.allocated_real_regs.is_reserved(scratch_reg) {
                    self.allocated_real_regs += scratch_reg;
                    self.set_stored_mapping(any_reg, scratch_reg);
                    // if unsafe { BLOCK_LOG } {
                    //     println!("allocate {any_reg} scratch {scratch_reg:?}");
                    // }
                    return scratch_reg;
                }
            }
        }

        for allocatable_reg in ALLOCATION_REGS {
            if !self.allocated_real_regs.is_reserved(allocatable_reg) {
                self.allocated_real_regs += allocatable_reg;
                self.set_stored_mapping(any_reg, allocatable_reg);
                // if unsafe { BLOCK_LOG } {
                //     println!("allocate {any_reg} allocatable {allocatable_reg:?}");
                // }
                return allocatable_reg;
            }
        }

        // Swap with a reg that is no longer used in the basic block
        let first_live_range = *live_ranges.first().unwrap();
        let unused_regs_in_block = !(first_live_range + current_outputs);
        for i in 0..self.stored_mapping_reverse.len() {
            if let Some(unused_any_reg) = self.stored_mapping_reverse[i] {
                let stored_mapping = Reg::from(i as u8);
                if unused_regs_in_block.contains(BlockReg::Any(unused_any_reg)) && !fixed_regs_used.is_reserved(stored_mapping) {
                    self.swap_stored_mapping(any_reg, unused_any_reg);
                    // if unsafe { BLOCK_LOG } {
                    //     println!("allocate {any_reg} unused in block {stored_mapping:?}, {unused_any_reg}");
                    // }
                    return stored_mapping;
                }
            }
        }

        // Spill a reg that will not be used in near future
        let mut max_index = 0;
        let mut max_used_any_reg = 0;
        let currently_used = *used_regs.first().unwrap();
        for i in 0..self.stored_mapping_reverse.len() {
            if let Some(used_any_reg) = self.stored_mapping_reverse[i] {
                if !currently_used.contains(BlockReg::Any(used_any_reg)) {
                    for j in 1..used_regs.len() {
                        if used_regs[j].contains(BlockReg::Any(used_any_reg)) {
                            if j > max_index {
                                max_index = j;
                                max_used_any_reg = used_any_reg;
                            }
                            break;
                        }
                    }
                }
            }
        }

        if max_index != 0 {
            let stored_mapping = self.stored_mapping[max_used_any_reg as usize];
            self.spilled.insert(max_used_any_reg);
            self.spilled.remove(&any_reg);
            self.swap_stored_mapping(any_reg, max_used_any_reg);
            self.gen_pre_handle_spilled_inst(max_used_any_reg, stored_mapping, BlockTransferOp::Write);
            // if unsafe { BLOCK_LOG } {
            //     println!("spill reg not used in near future {max_used_any_reg} {stored_mapping:?} for {any_reg}");
            // }
            return stored_mapping;
        }

        // Just spill any reg that is currently not needed
        let currently_not_used_regs = !*used_regs.first().unwrap();
        for i in 0..self.stored_mapping_reverse.len() {
            if let Some(used_any_reg) = self.stored_mapping_reverse[i] {
                if currently_not_used_regs.contains(BlockReg::Any(used_any_reg)) {
                    let stored_mapping = Reg::from(i as u8);
                    self.spilled.insert(used_any_reg);
                    self.spilled.remove(&any_reg);
                    self.swap_stored_mapping(any_reg, used_any_reg);
                    self.gen_pre_handle_spilled_inst(used_any_reg, stored_mapping, BlockTransferOp::Write);
                    // if unsafe { BLOCK_LOG } {
                    //     println!("spill currently not used reg {used_any_reg} {stored_mapping:?} for {any_reg}");
                    // }
                    return stored_mapping;
                }
            }
        }

        unreachable!()
    }

    fn deallocate_reg(&mut self, fixed_reg: Reg, live_ranges: &[BlockRegSet]) {
        // if unsafe { BLOCK_LOG } {
        //     println!("deallocate {fixed_reg:?}");
        // }
        let subsequent_live_range = &live_ranges[1];
        if !subsequent_live_range.contains(BlockReg::Fixed(fixed_reg)) {
            self.allocated_real_regs -= fixed_reg;
        }
        if let Some(any_reg) = self.stored_mapping_reverse[fixed_reg as usize] {
            if live_ranges.first().unwrap().contains(BlockReg::Any(any_reg)) {
                // if unsafe { BLOCK_LOG } {
                //     println!("spill {any_reg} caused by deallocation");
                // }
                self.spilled.insert(any_reg);
                self.gen_pre_handle_spilled_inst(any_reg, fixed_reg, BlockTransferOp::Write);
            }
            self.remove_stored_mapping(any_reg);
        }
    }

    fn get_input_reg(&mut self, input_any_reg: u16, current_outputs: BlockRegSet, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) -> Reg {
        let stored_mapping = self.stored_mapping[input_any_reg as usize];
        if stored_mapping == Reg::None {
            if self.spilled.contains(&input_any_reg) {
                // if unsafe { BLOCK_LOG } {
                //     println!("input restore spilled {input_any_reg}")
                // }
                let stored_mapping = self.allocate_reg(input_any_reg, current_outputs, live_ranges, used_regs);
                self.gen_pre_handle_spilled_inst(input_any_reg, stored_mapping, BlockTransferOp::Read);
                return stored_mapping;
            } else {
                panic!("input reg {input_any_reg} must be allocated")
            }
        }
        stored_mapping
    }

    fn get_output_reg(&mut self, output_any_reg: u16, current_outputs: BlockRegSet, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) -> Reg {
        let stored_mapping = self.stored_mapping[output_any_reg as usize];
        if stored_mapping == Reg::None {
            return self.allocate_reg(output_any_reg, current_outputs, live_ranges, used_regs);
        }
        stored_mapping
    }

    pub fn inst_allocate(&mut self, inst: &mut BlockInst, live_ranges: &[BlockRegSet], used_regs: &[BlockRegSet]) {
        self.pre_allocate_insts.clear();

        let (inputs, outputs) = inst.get_io();

        // if unsafe { BLOCK_LOG } {
        //     println!("{inst:?}");
        //     println!("\tinputs: {inputs:?}, outputs: {outputs:?}");
        //     println!("\tlive range: {:?}", live_ranges[0]);
        // }

        if inputs.is_empty() && outputs.is_empty() {
            return;
        }

        for input_any_reg in inputs.iter_any() {
            // if unsafe { BLOCK_LOG } {
            //     println!("input {input_any_reg}");
            // }
            let allocated_input_reg = self.get_input_reg(input_any_reg, outputs, live_ranges, used_regs);
            inst.replace_input_regs(BlockReg::Any(input_any_reg), BlockReg::Fixed(allocated_input_reg));
        }

        // if unsafe { BLOCK_LOG } {
        //     for (any_reg, stored_mapping) in self.stored_mapping.iter().enumerate() {
        //         if *stored_mapping != Reg::None {
        //             assert_eq!(self.stored_mapping_reverse[*stored_mapping as usize], Some(any_reg as u16));
        //         }
        //     }
        //
        //     for (i, reg) in self.stored_mapping_reverse.iter().enumerate() {
        //         if let Some(reg) = reg {
        //             assert_eq!(self.stored_mapping[*reg as usize], Reg::from(i as u8), "{reg:?}");
        //         }
        //     }
        // }

        for output_fixed_reg in outputs.iter_fixed() {
            // if unsafe { BLOCK_LOG } {
            //     println!("fixed output {output_fixed_reg:?}");
            // }
            if self.allocated_real_regs.is_reserved(output_fixed_reg) {
                self.deallocate_reg(output_fixed_reg, live_ranges);
            }
        }

        // if unsafe { BLOCK_LOG } {
        //     for (any_reg, stored_mapping) in self.stored_mapping.iter().enumerate() {
        //         if *stored_mapping != Reg::None {
        //             assert_eq!(self.stored_mapping_reverse[*stored_mapping as usize], Some(any_reg as u16));
        //         }
        //     }
        //
        //     for (i, reg) in self.stored_mapping_reverse.iter().enumerate() {
        //         if let Some(reg) = reg {
        //             assert_eq!(self.stored_mapping[*reg as usize], Reg::from(i as u8), "{reg:?}");
        //         }
        //     }
        // }

        for output_any_reg in outputs.iter_any() {
            // if unsafe { BLOCK_LOG } {
            //     println!("output {output_any_reg}");
            // }
            let allocated_output_reg = self.get_output_reg(output_any_reg, outputs, live_ranges, used_regs);
            inst.replace_output_regs(BlockReg::Any(output_any_reg), BlockReg::Fixed(allocated_output_reg));
        }

        // for (any_reg, stored_mapping) in self.stored_mapping.iter().enumerate() {
        //     if *stored_mapping != Reg::None {
        //         assert_eq!(self.stored_mapping_reverse[*stored_mapping as usize], Some(any_reg as u16), "{stored_mapping:?}");
        //     }
        // }
        //
        // for (i, reg) in self.stored_mapping_reverse.iter().enumerate() {
        //     if let Some(reg) = reg {
        //         assert_eq!(self.stored_mapping[*reg as usize], Reg::from(i as u8), "{reg:?}");
        //     }
        // }
    }

    pub fn ensure_global_regs_mapping(&mut self, outputs: BlockRegSet) {
        self.pre_allocate_insts.clear();

        for output_reg in outputs.iter_any() {
            match self.global_regs_mapping.get(&output_reg).unwrap() {
                Reg::None => {
                    // if unsafe { BLOCK_LOG } {
                    //     println!("ensure {output_reg} is spilled");
                    // }
                    let stored_mapping = self.stored_mapping[output_reg as usize];
                    if stored_mapping != Reg::None {
                        self.remove_stored_mapping(output_reg);
                        self.allocated_real_regs -= stored_mapping;
                        self.spilled.insert(output_reg);
                        self.gen_pre_handle_spilled_inst(output_reg, stored_mapping, BlockTransferOp::Write);
                    }
                }
                desired_reg_mapping => {
                    let desired_reg_mapping = *desired_reg_mapping;

                    let stored_mapping = self.stored_mapping[output_reg as usize];
                    // if unsafe { BLOCK_LOG } {
                    //     println!("ensure {output_reg} has desired {desired_reg_mapping:?} but has {stored_mapping:?}");
                    // }
                    if desired_reg_mapping == stored_mapping {
                        // Already at correct register, skip
                        continue;
                    }

                    if let Some(currently_used_by) = self.stored_mapping_reverse[desired_reg_mapping as usize] {
                        // Some other any reg is using the desired reg
                        // if unsafe { BLOCK_LOG } {
                        //     println!("ensure currently used {currently_used_by}");
                        // }

                        if outputs.contains(BlockReg::Any(currently_used_by)) {
                            // other any reg is part of required output
                            match self.global_regs_mapping.get(&currently_used_by).unwrap() {
                                Reg::None => {
                                    // other any reg is part of predetermined spilled
                                    // if unsafe { BLOCK_LOG } {
                                    //     println!("ensure spill currently used {currently_used_by}");
                                    // }
                                    self.remove_stored_mapping(currently_used_by);
                                    self.spilled.insert(currently_used_by);
                                    self.gen_pre_handle_spilled_inst(currently_used_by, desired_reg_mapping, BlockTransferOp::Write);
                                }
                                _ => {
                                    let mut moved = false;
                                    // find a mapped any reg that is not part of output for back up
                                    for (i, unused_reg_mapped) in self.stored_mapping_reverse.iter().enumerate() {
                                        if let Some(unused_reg_mapped) = unused_reg_mapped {
                                            if !outputs.contains(BlockReg::Any(*unused_reg_mapped)) {
                                                let stored_mapping = Reg::from(i as u8);
                                                // if unsafe { BLOCK_LOG } {
                                                //     println!("ensure remove unused {unused_reg_mapped} with {stored_mapping:?}");
                                                // }
                                                self.remove_stored_mapping(*unused_reg_mapped);
                                                self.set_stored_mapping(currently_used_by, stored_mapping);
                                                self.pre_allocate_insts.push(BlockInst::Alu2Op0 {
                                                    op: BlockAluOp::Mov,
                                                    operands: [BlockReg::Fixed(stored_mapping).into(), BlockReg::Fixed(desired_reg_mapping).into()],
                                                    set_cond: BlockAluSetCond::None,
                                                    thumb_pc_aligned: false,
                                                });
                                                moved = true;
                                                break;
                                            }
                                        }
                                    }

                                    if !moved {
                                        // no unused any reg found, just spill the any reg using the desired reg
                                        // if unsafe { BLOCK_LOG } {
                                        //     println!("ensure spilled currently used {currently_used_by}");
                                        // }
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
                        // if unsafe { BLOCK_LOG } {
                        //     println!("ensure mov {output_reg} {stored_mapping:?} to {desired_reg_mapping:?}")
                        // }
                        self.remove_stored_mapping(output_reg);
                        self.allocated_real_regs -= stored_mapping;
                        self.pre_allocate_insts.push(BlockInst::Alu2Op0 {
                            op: BlockAluOp::Mov,
                            operands: [BlockReg::Fixed(desired_reg_mapping).into(), BlockReg::Fixed(stored_mapping).into()],
                            set_cond: BlockAluSetCond::None,
                            thumb_pc_aligned: false,
                        });
                    } else if self.spilled.contains(&output_reg) {
                        // if unsafe { BLOCK_LOG } {
                        //     println!("ensure restore {output_reg} to {desired_reg_mapping:?}")
                        // }
                        self.spilled.remove(&output_reg);
                        self.gen_pre_handle_spilled_inst(output_reg, desired_reg_mapping, BlockTransferOp::Read);
                    } else {
                        panic!("required output reg must already have a value");
                    }
                    self.set_stored_mapping(output_reg, desired_reg_mapping);
                    self.allocated_real_regs += desired_reg_mapping;
                }
            }

            // for (any_reg, stored_mapping) in self.stored_mapping.iter().enumerate() {
            //     if *stored_mapping != Reg::None {
            //         assert_eq!(self.stored_mapping_reverse[*stored_mapping as usize], Some(any_reg as u16));
            //     }
            // }
            //
            // for (i, reg) in self.stored_mapping_reverse.iter().enumerate() {
            //     if let Some(reg) = reg {
            //         assert_eq!(self.stored_mapping[*reg as usize], Reg::from(i as u8), "{reg:?}");
            //     }
            // }
        }
    }
}

use crate::jit::assembler::block_inst::BlockInst;
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use std::slice;

const ALLOCATION_REGS: RegReserve = reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11);
const SCRATCH_REGS: RegReserve = reg_reserve!(Reg::R0, Reg::R1, Reg::R2, Reg::R3, Reg::R12);

pub struct BlockRegAllocator {
    allocated_real_regs: RegReserve,
    stored_mapping: [Reg; ANY_REG_LIMIT as usize],         // mappings to real registers
    spilled_mapping: [Option<u8>; ANY_REG_LIMIT as usize], // contains stack offsets
    allocation_insts: Vec<BlockInst>,
}

impl BlockRegAllocator {
    pub fn new() -> Self {
        BlockRegAllocator {
            allocated_real_regs: RegReserve::new(),
            stored_mapping: [Reg::None; ANY_REG_LIMIT as usize],
            spilled_mapping: [None; ANY_REG_LIMIT as usize],
            allocation_insts: Vec::new(),
        }
    }

    fn allocate_reg(&mut self, any_reg: u8, live_ranges: &[BlockRegSet], inst_fixed_outputs: &[RegReserve]) -> Reg {
        let mut live_expired = false;
        let mut fixed_regs_used = RegReserve::new();
        for i in 1..live_ranges.len() {
            fixed_regs_used += inst_fixed_outputs[i];
            if !live_ranges[i].contains(BlockReg::Any(any_reg)) {
                live_expired = true;
                break;
            }
        }

        if live_expired {
            for scratch_reg in SCRATCH_REGS {
                if !fixed_regs_used.is_reserved(scratch_reg) && !self.allocated_real_regs.is_reserved(scratch_reg) {
                    self.allocated_real_regs += scratch_reg;
                    self.stored_mapping[any_reg as usize] = scratch_reg;
                    return scratch_reg;
                }
            }
        }

        for allocatable_reg in ALLOCATION_REGS {
            if !self.allocated_real_regs.is_reserved(allocatable_reg) {
                self.allocated_real_regs += allocatable_reg;
                self.stored_mapping[any_reg as usize] = allocatable_reg;
                return allocatable_reg;
            }
        }

        for dead_any_reg in (!live_ranges[0]).iter_any() {
            let stored_mapping = self.stored_mapping[dead_any_reg as usize];
            if stored_mapping != Reg::None {
                self.stored_mapping.swap(any_reg as usize, dead_any_reg as usize);
                return stored_mapping;
            }
        }

        todo!()
    }

    fn deallocate_reg(&mut self, fixed_reg: Reg) {
        self.allocated_real_regs -= fixed_reg;
        for i in 0..self.stored_mapping.len() {
            if self.stored_mapping[i] == fixed_reg {
                self.stored_mapping[i] = Reg::None;
                if !SCRATCH_REGS.is_reserved(fixed_reg) {
                    todo!()
                }
            }
        }
    }

    fn get_input_reg(&mut self, input_any_reg: u8, live_ranges: &[BlockRegSet]) -> Reg {
        let stored_mapping = self.stored_mapping[input_any_reg as usize];
        if stored_mapping == Reg::None {
            match self.spilled_mapping[input_any_reg as usize] {
                None => panic!("input reg {input_any_reg} must be allocated"),
                Some(stack_offset) => {
                    todo!()
                }
            }
        }
        stored_mapping
    }

    fn get_output_reg(&mut self, output_any_reg: u8, live_ranges: &[BlockRegSet], inst_fixed_outputs: &[RegReserve]) -> Reg {
        let stored_mapping = self.stored_mapping[output_any_reg as usize];
        if stored_mapping == Reg::None {
            match self.spilled_mapping[output_any_reg as usize] {
                None => return self.allocate_reg(output_any_reg, live_ranges, inst_fixed_outputs),
                Some(stack_offset) => {
                    todo!()
                }
            }
        }
        stored_mapping
    }

    pub fn inst_allocate(&mut self, inst: &mut BlockInst, live_ranges: &[BlockRegSet], inst_fixed_outputs: &[RegReserve]) -> usize {
        self.allocation_insts.clear();

        let (inputs, outputs) = inst.get_io();
        for input_any_reg in inputs.iter_any() {
            inst.replace_regs(BlockReg::Any(input_any_reg), BlockReg::Fixed(self.get_input_reg(input_any_reg, live_ranges)));
        }

        for output_fixed_reg in outputs.iter_fixed() {
            if self.allocated_real_regs.is_reserved(output_fixed_reg) {
                self.deallocate_reg(output_fixed_reg);
            }
        }

        let load_insts_size = self.allocation_insts.len();
        for output_any_reg in outputs.iter_any() {
            inst.replace_regs(BlockReg::Any(output_any_reg), BlockReg::Fixed(self.get_output_reg(output_any_reg, live_ranges, inst_fixed_outputs)));
        }
        load_insts_size
    }

    pub fn ensure_mapping(&mut self, regs: &BlockRegSet) {
        for any_reg in regs.iter_any() {
            self.get_output_reg(any_reg, slice::from_ref(regs), slice::from_ref(&SCRATCH_REGS));
        }
    }
}

use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{reg_reserve, RegReserve};
use crate::jit::Cond;
use std::fmt::{Debug, Formatter};

pub struct BasicBlock {
    pub start_pc: u32,
    pub start_index: usize,
    pub end_index: usize,
    pub live_regs: Vec<RegReserve>,
    pub output_regs: RegReserve,
}

impl BasicBlock {
    pub fn new(start_pc: u32, start_index: usize, end_index: usize) -> Self {
        BasicBlock {
            start_pc,
            start_index,
            end_index,
            live_regs: Vec::new(),
            output_regs: reg_reserve!(),
        }
    }

    pub fn debug<'a>(&'a self, insts: &'a [InstInfo], thumb: bool) -> BasicBlockDebug<'a> {
        BasicBlockDebug { basic_block: self, insts, thumb }
    }

    pub fn get_inputs(&self) -> RegReserve {
        self.live_regs[0]
    }

    pub fn resolve_live_regs(&mut self, insts: &[InstInfo]) {
        let inst_length = self.end_index - self.start_index + 1;
        self.live_regs.clear();
        self.live_regs.resize(inst_length + 1, RegReserve::new());

        for i in (0..inst_length).rev() {
            let inst = &insts[i + self.start_index];
            let mut previous_ranges = self.live_regs[i + 1];
            previous_ranges -= inst.out_regs;
            self.live_regs[i] = previous_ranges + inst.src_regs;
            self.output_regs += inst.out_regs;
            if inst.cond != Cond::AL {
                self.live_regs[i] += inst.out_regs;
            }
        }
    }
}

pub struct BasicBlockDebug<'a> {
    basic_block: &'a BasicBlock,
    insts: &'a [InstInfo],
    thumb: bool,
}

impl Debug for BasicBlockDebug<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let pc_shift = if self.thumb { 1 } else { 2 };
        for i in self.basic_block.start_index..self.basic_block.end_index + 1 {
            let inst = &self.insts[i];
            let i = i - self.basic_block.start_index;
            let pc = self.basic_block.start_pc + ((i as u32) << pc_shift);
            writeln!(f, "{pc:x}: live regs: {:?}", self.basic_block.live_regs[i])?;
            writeln!(f, "{inst:?}")?;
        }
        write!(f, "outputs: {:?}", self.basic_block.live_regs.last().unwrap())
    }
}

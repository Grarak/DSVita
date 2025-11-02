use crate::jit::analyzer::basic_block::BasicBlock;
use crate::jit::inst_info::InstInfo;
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use crate::logging::block_asm_println;
use crate::utils::NoHashSet;
use bilge::prelude::*;

pub enum JitBranchInfo {
    Idle(usize),
    Local(usize),
    None,
}

// Taken from https://github.com/melonDS-emu/melonDS/blob/24c402af51fe9c0537582173fc48d1ad3daff459/src/ARMJIT.cpp#L352
fn is_idle_loop(insts: &[InstInfo]) -> bool {
    let mut regs_written_to = RegReserve::new();
    let mut regs_disallowed_to_write = RegReserve::new();
    for inst in &insts[..insts.len() - 1] {
        if inst.is_branch()
            || matches!(
                inst.op,
                Op::Swi | Op::SwiT | Op::Mcr | Op::Mrc | Op::MrsRc | Op::MrsRs | Op::MsrIc | Op::MsrIs | Op::MsrRc | Op::MsrRs | Op::Swp | Op::Swpb
            )
            || inst.op.is_write_mem_transfer()
        {
            return false;
        }

        let src_regs = inst.src_regs - reg_reserve!(Reg::PC, Reg::CPSR);
        let out_regs = inst.out_regs - reg_reserve!(Reg::PC);
        regs_disallowed_to_write += src_regs - regs_written_to;

        if !(out_regs & regs_disallowed_to_write).is_empty() {
            return false;
        }
        regs_written_to += out_regs;
    }
    true
}

fn analyze_branch_label(insts: &[InstInfo], thumb: bool, branch_index: usize, cond: Cond, pc: u32, target_pc: u32) -> JitBranchInfo {
    if (cond as u8) < (Cond::AL as u8) && target_pc < pc {
        let diff = (pc - target_pc) >> if thumb { 1 } else { 2 };
        if diff as usize <= branch_index {
            let jump_to_index = branch_index - diff as usize;
            if is_idle_loop(&insts[jump_to_index..branch_index + 1]) {
                return JitBranchInfo::Idle(jump_to_index);
            }
        }
    }

    let relative_index = (target_pc as i32 - pc as i32) >> if thumb { 1 } else { 2 };
    let target_index = branch_index as i32 + relative_index;
    if target_index >= 0 && (target_index as usize) < insts.len() {
        JitBranchInfo::Local(target_index as usize)
    } else {
        JitBranchInfo::None
    }
}

#[bitsize(8)]
#[derive(Copy, Clone, FromBits)]
pub struct InstMetadata {
    pub idle_loop: bool,
    pub external_branch: bool,
    pub local_branch_entry: bool,
    not_used: u5,
}

impl Default for InstMetadata {
    fn default() -> Self {
        InstMetadata::from(0)
    }
}

#[derive(Default)]
pub struct AsmAnalyzer {
    thumb: bool,
    pub basic_blocks: Vec<BasicBlock>,
    pub insts_metadata: Vec<InstMetadata>,
    imm_store_addrs: NoHashSet<u32>,
}

impl AsmAnalyzer {
    fn create_basic_blocks(&mut self, start_pc: u32, insts: &[InstInfo]) {
        self.basic_blocks.clear();
        self.insts_metadata.clear();
        self.insts_metadata.resize(insts.len(), InstMetadata::default());
        self.imm_store_addrs.clear();

        let pc_shift = if self.thumb { 1 } else { 2 };
        for i in 0..insts.len() {
            let pc = start_pc + ((i as u32) << pc_shift);
            if let Some(imm_addr) = insts[i].imm_transfer_addr(pc) {
                if insts[i].op.is_write_mem_transfer() {
                    self.imm_store_addrs.insert(imm_addr);
                }
            }

            if insts[i].op.is_labelled_branch() && !insts[i].out_regs.is_reserved(Reg::LR) {
                let relative_pc = insts[i].operands()[0].as_imm().unwrap() as i32 + (2 << pc_shift);
                let target_pc = (pc as i32 + relative_pc) as u32;

                match analyze_branch_label(insts, self.thumb, i, insts[i].cond, pc, target_pc) {
                    JitBranchInfo::Idle(target_index) => {
                        self.insts_metadata[i].set_idle_loop(true);
                        self.insts_metadata[target_index].set_local_branch_entry(true);
                    }
                    JitBranchInfo::Local(target_index) => self.insts_metadata[target_index].set_local_branch_entry(true),
                    JitBranchInfo::None => self.insts_metadata[i].set_external_branch(true),
                }
            }
        }

        let mut block_start = 0;
        for i in 0..insts.len() {
            if self.insts_metadata[i].local_branch_entry() {
                if i > block_start {
                    self.basic_blocks.push(BasicBlock::new(start_pc + ((block_start as u32) << pc_shift), block_start, i - 1));
                }
                block_start = i;
            }

            if insts[i].op.is_labelled_branch() && !insts[i].out_regs.is_reserved(Reg::LR) {
                self.basic_blocks.push(BasicBlock::new(start_pc + ((block_start as u32) << pc_shift), block_start, i));
                block_start = i + 1;
            }
        }
        if block_start < insts.len() {
            self.basic_blocks.push(BasicBlock::new(start_pc + ((block_start as u32) << pc_shift), block_start, insts.len() - 1));
        }

        for basic_block in &mut self.basic_blocks {
            basic_block.resolve_live_regs(insts);
        }
    }

    pub fn get_basic_block_metadata(&self, basic_block_index: usize) -> InstMetadata {
        self.insts_metadata[self.basic_blocks[basic_block_index].start_index]
    }

    pub fn get_next_live_regs(&self, basic_block_index: usize, inst_index: usize) -> RegReserve {
        let basic_block = &self.basic_blocks[basic_block_index];
        basic_block.live_regs[inst_index - basic_block.start_index + 1]
    }

    pub fn get_basic_block_from_inst(&self, inst_index: usize) -> usize {
        for (i, basic_block) in self.basic_blocks.iter().enumerate() {
            if inst_index >= basic_block.start_index && inst_index <= basic_block.end_index {
                return i;
            }
        }
        unreachable!()
    }

    pub fn get_pc_from_inst(&self, inst_index: usize) -> u32 {
        let pc_shift = if self.thumb { 1 } else { 2 };
        self.basic_blocks[0].start_pc + ((inst_index as u32) << pc_shift)
    }

    pub fn can_imm_load(&self, guest_addr: u32) -> bool {
        !self.imm_store_addrs.contains(&guest_addr)
    }

    pub fn analyze(&mut self, start_pc: u32, insts: &[InstInfo], thumb: bool) {
        self.thumb = thumb;
        self.create_basic_blocks(start_pc, insts);

        for (i, basic_block) in self.basic_blocks.iter().enumerate() {
            block_asm_println!("basic block {i} start inst {} - {}", basic_block.start_index, basic_block.end_index);
            block_asm_println!("{:?}", basic_block.debug(insts, thumb));
            block_asm_println!("basic block {i} end");
        }
    }
}

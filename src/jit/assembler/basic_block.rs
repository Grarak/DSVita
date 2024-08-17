use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::block_inst::{BlockAluOp, BlockTransferOp};
use crate::jit::assembler::block_reg_allocator::BlockRegAllocator;
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockInst, BlockReg};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::MemoryAmount;
use crate::utils::NoHashSet;
use std::fmt::{Debug, Formatter};

pub struct BasicBlock {
    pub start_asm_inst: usize,
    pub end_asm_inst: usize,
    pub regs_live_ranges: Vec<BlockRegSet>,
    inst_fixed_outputs: Vec<RegReserve>,
    pub regs_output: BlockRegSet,
    pub enter_blocks: NoHashSet<usize>,
    pub exit_blocks: Vec<usize>,
    pub insts: Vec<BlockInst>,
    used_input_guest_regs_in_block: RegReserve,
}

impl BasicBlock {
    pub fn new(start_asm_inst: usize, end_asm_inst: usize) -> Self {
        BasicBlock {
            start_asm_inst,
            end_asm_inst,
            regs_live_ranges: Vec::new(),
            inst_fixed_outputs: Vec::new(),
            regs_output: BlockRegSet::new(),
            enter_blocks: NoHashSet::default(),
            exit_blocks: Vec::with_capacity(2),
            insts: Vec::new(),
            used_input_guest_regs_in_block: RegReserve::new(),
        }
    }

    pub fn init_resolve_guest_regs(&mut self, asm: &mut BlockAsm, guest_regs_written_to: &mut RegReserve, guest_regs_mapping: &[Option<BlockReg>; Reg::SPSR as usize]) {
        for inst in &mut asm.buf.insts[self.start_asm_inst..=self.end_asm_inst] {
            if let BlockInst::SaveContext { regs_to_save, .. } = inst {
                for reg in guest_regs_written_to.into_iter() {
                    regs_to_save[reg as usize] = guest_regs_mapping[reg as usize];
                }
                guest_regs_written_to.clear();
            } else {
                let (inputs, outputs) = inst.get_io();
                *guest_regs_written_to += outputs.get_guests();
                self.used_input_guest_regs_in_block += inputs.get_guests();

                for (i, mapped_reg) in guest_regs_mapping.iter().enumerate() {
                    if let Some(mapped_reg) = mapped_reg {
                        inst.replace_regs(BlockReg::Guest(Reg::from(i as u8)), *mapped_reg);
                    }
                }
            }
        }
    }

    pub fn init_insts(&mut self, asm: &BlockAsm, guest_regs_mapping: &[Option<BlockReg>; Reg::SPSR as usize], pc: u32) {
        self.insts.reserve(self.used_input_guest_regs_in_block.len() + (self.end_asm_inst - self.start_asm_inst + 1));
        for reg in self.used_input_guest_regs_in_block {
            if reg == Reg::PC {
                self.insts.push(BlockInst::Alu2Op0 {
                    op: BlockAluOp::Mov,
                    operands: [guest_regs_mapping[reg as usize].unwrap().into(), pc.into()],
                })
            } else {
                self.insts.push(BlockInst::Transfer {
                    op: BlockTransferOp::Read,
                    operands: [guest_regs_mapping[reg as usize].unwrap().into(), asm.thread_regs_addr_reg.into(), (reg as u32 * 4).into()],
                    signed: false,
                    amount: MemoryAmount::Word,
                })
            }
        }

        self.insts.extend(&asm.buf.insts[self.start_asm_inst..=self.end_asm_inst]);
    }

    pub fn init_io(&mut self, required_outputs: BlockRegSet) {
        self.regs_live_ranges.clear();
        self.regs_live_ranges.resize(self.insts.len(), BlockRegSet::new());

        self.inst_fixed_outputs.clear();
        self.inst_fixed_outputs.resize(self.insts.len(), RegReserve::new());

        let (last_inst_inputs, last_inst_outputs) = self.insts.last().unwrap().get_io();
        self.regs_output += required_outputs + last_inst_outputs;
        *self.regs_live_ranges.last_mut().unwrap() = self.regs_output - last_inst_outputs + last_inst_inputs;
        *self.inst_fixed_outputs.last_mut().unwrap() = last_inst_outputs.get_fixed();

        for (i, inst) in self.insts[..self.insts.len() - 1].iter().enumerate().rev() {
            let (inputs, outputs) = inst.get_io();
            let mut previous_ranges = self.regs_live_ranges[i + 1];
            previous_ranges -= outputs;
            self.regs_live_ranges[i] = previous_ranges + inputs;
            self.inst_fixed_outputs[i] += outputs.get_fixed();
        }
    }

    pub fn get_required_inputs(&self) -> BlockRegSet {
        *self.regs_live_ranges.first().unwrap()
    }

    pub fn emit_opcodes(mut self, reg_allocator: &mut BlockRegAllocator, branch_placeholders: &mut Vec<usize>, opcodes_offset: usize) -> Vec<u32> {
        for (i, inst) in self.insts.iter_mut().enumerate() {
            reg_allocator.inst_allocate(inst, &self.regs_live_ranges[i..], &self.inst_fixed_outputs[i..]);
        }

        reg_allocator.ensure_mapping(&self.regs_output);

        let mut opcodes = Vec::new();
        let mut inst_opcodes = Vec::new();
        for inst in self.insts {
            inst_opcodes.clear();
            inst.emit_opcode(&mut inst_opcodes, opcodes.len(), branch_placeholders, opcodes_offset);
            opcodes.extend(&inst_opcodes);
        }
        opcodes
    }
}

impl Debug for BasicBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "BasicBlock: inputs: {:?} enter blocks: {:?}", self.regs_live_ranges.first().unwrap(), self.enter_blocks)?;
        for (i, inst) in self.insts.iter().enumerate() {
            writeln!(f, "\t{inst:?}")?;
            writeln!(f, "\t\tinputs: {:?}", self.regs_live_ranges[i])?;
        }
        write!(f, "BasicBlock end: outputs: {:?} exit blocks: {:?}", self.regs_output, self.exit_blocks)
    }
}

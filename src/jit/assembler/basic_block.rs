use crate::jit::assembler::block_asm::BlockAsm;
// use crate::jit::assembler::block_asm::BLOCK_LOG;
use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockSystemRegOp, BlockTransferOp};
use crate::jit::assembler::block_reg_allocator::BlockRegAllocator;
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::BlockInst;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, ShiftType};
use crate::utils::{BuildNoHasher, NoHashSet};
use std::fmt::{Debug, Formatter};

pub struct BasicBlock {
    pub start_asm_inst: usize,
    pub end_asm_inst: usize,

    pub regs_live_ranges: Vec<BlockRegSet>,
    pub used_regs: Vec<BlockRegSet>,

    pub enter_blocks: NoHashSet<usize>,
    pub exit_blocks: NoHashSet<usize>,

    pub enter_blocks_guest_resolved: NoHashSet<usize>,
    pub guest_regs_dirty: RegReserve,
    needs_guest_pc: bool,

    pub exit_blocks_io_resolved: NoHashSet<usize>,

    pub insts: Vec<BlockInst>,

    pub cond_block: Cond,
}

impl BasicBlock {
    pub fn new(start_asm_inst: usize, end_asm_inst: usize) -> Self {
        BasicBlock {
            start_asm_inst,
            end_asm_inst,

            regs_live_ranges: Vec::new(),
            used_regs: Vec::new(),

            enter_blocks: NoHashSet::default(),
            exit_blocks: NoHashSet::with_capacity_and_hasher(2, BuildNoHasher),

            enter_blocks_guest_resolved: NoHashSet::default(),
            guest_regs_dirty: RegReserve::new(),
            needs_guest_pc: false,

            exit_blocks_io_resolved: NoHashSet::with_capacity_and_hasher(2, BuildNoHasher),

            insts: Vec::new(),

            cond_block: Cond::AL,
        }
    }

    pub fn init_resolve_guest_regs(&mut self, asm: &mut BlockAsm) {
        for inst in &mut asm.buf.insts[self.start_asm_inst..=self.end_asm_inst] {
            match inst {
                BlockInst::SaveContext { regs_to_save, .. } => {
                    *regs_to_save += self.guest_regs_dirty;
                    self.guest_regs_dirty.clear();
                }
                BlockInst::SaveReg { guest_reg, .. } | BlockInst::RestoreReg { guest_reg, .. } => {
                    self.guest_regs_dirty -= *guest_reg;
                }
                _ => {
                    let (inputs, outputs) = inst.get_io();
                    self.guest_regs_dirty += outputs.get_guests();
                    self.needs_guest_pc |= inputs.contains(Reg::PC.into());
                }
            }
        }
    }

    pub fn init_insts<const THUMB: bool>(&mut self, asm: &mut BlockAsm, basic_block_start_pc: u32) {
        if self.needs_guest_pc {
            self.insts.push(BlockInst::Alu2Op0 {
                op: BlockAluOp::Mov,
                operands: [Reg::PC.into(), basic_block_start_pc.into()],
                set_cond: BlockAluSetCond::None,
                thumb_pc_aligned: false,
            });
        }

        // Adjust insts with PC reg usage
        let mut last_pc = basic_block_start_pc;
        let mut last_pc_reg = Reg::PC.into();
        for i in self.start_asm_inst..=self.end_asm_inst {
            match &mut asm.buf.insts[i] {
                BlockInst::Label { guest_pc, .. } => {
                    if let Some(pc) = guest_pc {
                        last_pc = pc.0;
                    }
                }
                BlockInst::GuestPc(pc) => last_pc = pc.0,
                _ => match &mut asm.buf.insts[i] {
                    BlockInst::SaveContext {
                        thread_regs_addr_reg,
                        tmp_guest_cpsr_reg,
                        regs_to_save,
                    } => {
                        // Unroll regs to save into individual save regs, easier on reg allocator later on
                        for reg_to_save in *regs_to_save {
                            self.insts.push(BlockInst::SaveReg {
                                guest_reg: reg_to_save,
                                reg_mapped: if reg_to_save == Reg::PC { last_pc_reg } else { reg_to_save.into() },
                                thread_regs_addr_reg: *thread_regs_addr_reg,
                                tmp_guest_cpsr_reg: *tmp_guest_cpsr_reg,
                            })
                        }
                        continue;
                    }
                    BlockInst::SaveReg { guest_reg: Reg::PC, reg_mapped, .. } => *reg_mapped = last_pc_reg,
                    _ => {
                        let (inputs, outputs) = asm.buf.insts[i].get_io();
                        if inputs.contains(Reg::PC.into()) {
                            let mut last_pc = last_pc + if THUMB { 4 } else { 8 };
                            if THUMB {
                                match &asm.buf.insts[i] {
                                    BlockInst::Alu3 { thumb_pc_aligned, .. }
                                    | BlockInst::Alu2Op1 { thumb_pc_aligned, .. }
                                    | BlockInst::Alu2Op0 { thumb_pc_aligned, .. }
                                    | BlockInst::Mul { thumb_pc_aligned, .. } => {
                                        if *thumb_pc_aligned {
                                            last_pc &= !0x3;
                                        }
                                    }
                                    _ => {}
                                }
                            } else if let BlockInst::GenericGuestInst { inst, .. } = &asm.buf.insts[i] {
                                // PC + 12 when ALU shift by register
                                if inst.op.is_alu_reg_shift() && *inst.operands().last().unwrap().as_reg().unwrap().0 == Reg::PC {
                                    last_pc += 4;
                                }
                            }
                            let pc_diff = last_pc - basic_block_start_pc;
                            self.insts.push(if pc_diff & !0xFF != 0 {
                                BlockInst::Alu2Op0 {
                                    op: BlockAluOp::Mov,
                                    operands: [asm.tmp_adjusted_pc_reg.into(), last_pc.into()],
                                    set_cond: BlockAluSetCond::None,
                                    thumb_pc_aligned: false,
                                }
                            } else {
                                BlockInst::Alu3 {
                                    op: BlockAluOp::Add,
                                    operands: [asm.tmp_adjusted_pc_reg.into(), Reg::PC.into(), pc_diff.into()],
                                    set_cond: BlockAluSetCond::None,
                                    thumb_pc_aligned: false,
                                }
                            });
                            asm.buf.insts[i].replace_regs(Reg::PC.into(), asm.tmp_adjusted_pc_reg);

                            if outputs.contains(Reg::PC.into()) {
                                last_pc_reg = asm.tmp_adjusted_pc_reg;
                            }
                        }

                        if inputs.contains(Reg::CPSR.into()) {
                            self.insts.push(BlockInst::SystemReg {
                                op: BlockSystemRegOp::Mrs,
                                operand: Reg::CPSR.into(),
                            });
                            self.insts.push(BlockInst::Transfer {
                                op: BlockTransferOp::Read,
                                operands: [asm.tmp_guest_cpsr_reg.into(), asm.thread_regs_addr_reg.into(), (Reg::CPSR as u32 * 4).into()],
                                signed: false,
                                amount: MemoryAmount::Word,
                                add_to_base: true,
                            });
                            self.insts.push(BlockInst::Alu3 {
                                op: BlockAluOp::And,
                                operands: [Reg::CPSR.into(), Reg::CPSR.into(), (0xF8, ShiftType::Ror, 4).into()],
                                set_cond: BlockAluSetCond::None,
                                thumb_pc_aligned: false,
                            });
                            self.insts.push(BlockInst::Alu3 {
                                op: BlockAluOp::Bic,
                                operands: [asm.tmp_guest_cpsr_reg.into(), asm.tmp_guest_cpsr_reg.into(), (0xF8, ShiftType::Ror, 4).into()],
                                set_cond: BlockAluSetCond::None,
                                thumb_pc_aligned: false,
                            });
                            self.insts.push(BlockInst::Alu3 {
                                op: BlockAluOp::Orr,
                                operands: [Reg::CPSR.into(), Reg::CPSR.into(), asm.tmp_guest_cpsr_reg.into()],
                                set_cond: BlockAluSetCond::None,
                                thumb_pc_aligned: false,
                            });
                        }

                        if inputs.contains(Reg::SPSR.into()) {
                            self.insts.push(BlockInst::Transfer {
                                op: BlockTransferOp::Read,
                                operands: [Reg::SPSR.into(), asm.thread_regs_addr_reg.into(), (Reg::SPSR as u32 * 4).into()],
                                signed: false,
                                amount: MemoryAmount::Word,
                                add_to_base: true,
                            });
                        }
                    }
                },
            }
            self.insts.push(asm.buf.insts[i].clone());
        }

        self.regs_live_ranges.resize(self.insts.len() + 1, BlockRegSet::new());
        self.used_regs.resize(self.insts.len() + 1, BlockRegSet::new());
    }

    pub fn init_resolve_io(&mut self) {
        for (i, inst) in self.insts.iter().enumerate().rev() {
            let (inputs, outputs) = inst.get_io();
            let mut previous_ranges = self.regs_live_ranges[i + 1];
            previous_ranges -= outputs;
            self.regs_live_ranges[i] = previous_ranges + inputs;
            self.used_regs[i] = inputs + outputs;
        }
    }

    pub fn add_required_outputs(&mut self, required_outputs: BlockRegSet) {
        *self.regs_live_ranges.last_mut().unwrap() += required_outputs;
        *self.used_regs.last_mut().unwrap() += required_outputs;
    }

    pub fn get_required_inputs(&self) -> &BlockRegSet {
        self.regs_live_ranges.first().unwrap()
    }

    pub fn emit_opcodes(mut self, reg_allocator: &mut BlockRegAllocator, branch_placeholders: &mut Vec<usize>, opcodes_offset: usize) -> Vec<u32> {
        let insts_len = self.insts.len();
        let mut i = 0;
        let mut offset_i = 0;
        let mut last_inst_opcode_len = 0;
        while i < insts_len {
            last_inst_opcode_len = offset_i;
            reg_allocator.inst_allocate(&mut self.insts[offset_i], &self.regs_live_ranges[i..], &self.used_regs[i..]);
            if !reg_allocator.pre_allocate_insts.is_empty() {
                self.insts.splice(offset_i..offset_i, reg_allocator.pre_allocate_insts.clone());
                offset_i += reg_allocator.pre_allocate_insts.len();
            }
            i += 1;
            offset_i += 1;
        }

        reg_allocator.ensure_global_regs_mapping(*self.regs_live_ranges.last().unwrap());
        // Make sure to restore mapping before a branch
        if let BlockInst::Branch { .. } = self.insts.last().unwrap() {
            self.insts.splice(last_inst_opcode_len..last_inst_opcode_len, reg_allocator.pre_allocate_insts.clone());
        } else {
            self.insts.extend_from_slice(&reg_allocator.pre_allocate_insts);
        }

        let mut opcodes = Vec::new();
        let mut inst_opcodes = Vec::new();
        for mut inst in self.insts {
            // match &inst {
            //     BlockInst::Label { guest_pc, .. } => {
            //         if let Some(pc) = guest_pc {
            //             if unsafe { BLOCK_LOG } {
            //                 println!("(0x{:x}, 0x{pc:?}),", opcodes_offset + opcodes.len());
            //             }
            //         }
            //     }
            //     BlockInst::GuestPc(pc) => {
            //         if unsafe { BLOCK_LOG } {
            //             println!("(0x{:x}, 0x{pc:?}),", opcodes_offset + opcodes.len());
            //         }
            //     }
            //     _ => {}
            // }

            inst_opcodes.clear();
            inst.emit_opcode(&mut inst_opcodes, opcodes.len(), branch_placeholders, opcodes_offset);
            opcodes.extend(&inst_opcodes);
        }

        if self.cond_block != Cond::AL {
            for opcode in &mut opcodes {
                *opcode = (*opcode & !(0xF << 28)) | ((self.cond_block as u32) << 28);
            }
        }
        opcodes
    }
}

impl Debug for BasicBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "BasicBlock: inputs: {:?} enter blocks: {:?}", self.regs_live_ranges.first(), self.enter_blocks)?;
        for (i, inst) in self.insts.iter().enumerate() {
            writeln!(f, "\t{inst:?}")?;
            let (inputs, outputs) = inst.get_io();
            writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
            writeln!(f, "\t\tlive range: {:?}", self.regs_live_ranges[i])?;
        }
        write!(f, "BasicBlock end: outputs: {:?} exit blocks: {:?}", self.regs_live_ranges.last(), self.exit_blocks)
    }
}

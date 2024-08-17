use crate::core::thread_regs::ThreadRegs;
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::basic_block::BasicBlock;
use crate::jit::assembler::block_inst::{BlockAluOp, BlockSystemRegOp, BlockTransferOp};
use crate::jit::assembler::block_reg_allocator::BlockRegAllocator;
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockAsmBuf, BlockInst, BlockLabel, BlockOperand, BlockOperandShift, BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount};
use crate::utils::{NoHashMap, NoHashSet};

macro_rules! alu3 {
    ($name:ident, $inst:ident) => {
        pub fn $name(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
            self.add_op3(BlockAluOp::$inst, op0.into(), op1.into(), op2.into())
        }
    };
}

macro_rules! alu2_op1 {
    ($name:ident, $inst:ident) => {
        pub fn $name(&mut self, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
            self.add_op2_op1(BlockAluOp::$inst, op1.into(), op2.into())
        }
    };
}

macro_rules! alu2_op0 {
    ($name:ident, $inst:ident) => {
        pub fn $name(&mut self, op0: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
            self.add_op2_op0(BlockAluOp::$inst, op0.into(), op2.into())
        }
    };
}

pub struct BlockAsm<'a> {
    pub buf: &'a mut BlockAsmBuf,
    any_reg_count: u8,
    label_count: u16,
    used_labels: NoHashSet<u16>,
    pub thread_regs_addr_reg: BlockReg,
    tmp_guest_cpsr_reg: BlockReg,
}

impl<'a> BlockAsm<'a> {
    pub fn new(thread_regs: &ThreadRegs, buf: &'a mut BlockAsmBuf) -> Self {
        buf.insts.clear();
        let thread_regs_addr_reg = BlockReg::Any(0);
        let tmp_guest_cpsr_reg = BlockReg::Any(1);
        let mut instance = BlockAsm {
            buf,
            any_reg_count: 2,
            label_count: 0,
            used_labels: NoHashSet::default(),
            thread_regs_addr_reg,
            tmp_guest_cpsr_reg,
        };
        instance.mov(thread_regs_addr_reg, thread_regs.get_reg_start_addr() as *const _ as u32);
        instance
    }

    pub fn new_reg(&mut self) -> BlockReg {
        assert!(self.any_reg_count < ANY_REG_LIMIT);
        let id = self.any_reg_count;
        self.any_reg_count += 1;
        BlockReg::Any(id)
    }

    pub fn new_label(&mut self) -> BlockLabel {
        assert!(self.label_count < u16::MAX);
        let id = self.label_count;
        self.label_count += 1;
        BlockLabel(id)
    }

    alu3!(add, Add);
    alu3!(bic, Bic);
    alu3!(sub, Sub);
    alu2_op1!(cmp, Cmp);
    alu2_op0!(mov, Mov);

    fn check_imm_shift_limit(&mut self, operand: &mut BlockOperandShift) {
        if operand.shift.value.needs_reg_for_imm(0x1F) {
            let reg = self.new_reg();
            self.mov(reg, operand.shift.value);
            operand.shift.value = reg.into();
        }
    }

    fn add_op3(&mut self, op: BlockAluOp, op0: BlockReg, op1: BlockReg, mut op2: BlockOperandShift) {
        if op2.operand.needs_reg_for_imm(0xFF) {
            let reg = self.new_reg();
            self.mov(reg, op2.operand);
            op2 = reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.buf.insts.push(BlockInst::Alu3 {
            op,
            operands: [op0.into(), op1.into(), op2],
        })
    }

    fn add_op2_op1(&mut self, op: BlockAluOp, op1: BlockReg, mut op2: BlockOperandShift) {
        if op2.operand.needs_reg_for_imm(0xFF) {
            let reg = self.new_reg();
            self.mov(reg, op2.operand);
            op2 = reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.buf.insts.push(BlockInst::Alu2Op1 { op, operands: [op1.into(), op2] })
    }

    fn add_op2_op0(&mut self, op: BlockAluOp, op0: BlockReg, mut op2: BlockOperandShift) {
        if op != BlockAluOp::Mov && op2.operand.needs_reg_for_imm(0xFF) {
            let reg = self.new_reg();
            self.mov(reg, op2.operand);
            op2 = reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.buf.insts.push(BlockInst::Alu2Op0 { op, operands: [op0.into(), op2] })
    }

    pub fn transfer_read(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        let mut op2 = op2.into();
        if op2.operand.needs_reg_for_imm(0xFFF) {
            let reg = self.new_reg();
            self.mov(reg, op2.operand);
            op2 = reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.buf.insts.push(BlockInst::Transfer {
            op: BlockTransferOp::Read,
            operands: [op0.into().into(), op1.into().into(), op2],
            signed,
            amount,
        })
    }

    pub fn mrs_cpsr(&mut self, operand: impl Into<BlockReg>) {
        self.buf.insts.push(BlockInst::SystemReg {
            op: BlockSystemRegOp::Mrs,
            operand: operand.into().into(),
        })
    }

    pub fn msr_cpsr(&mut self, operand: impl Into<BlockOperand>) {
        self.buf.insts.push(BlockInst::SystemReg {
            op: BlockSystemRegOp::Msr,
            operand: operand.into(),
        })
    }

    pub fn bfc(&mut self, operand: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.buf.insts.push(BlockInst::Bfc { operand: operand.into(), lsb, width })
    }

    pub fn label(&mut self, label: BlockLabel) {
        if !self.used_labels.insert(label.0) {
            panic!("{label:?} was already added");
        }
        self.buf.insts.push(BlockInst::Label(label))
    }

    pub fn branch(&mut self, label: BlockLabel, cond: Cond) {
        self.buf.insts.push(BlockInst::Branch { label, cond, block_index: 0 })
    }

    pub fn save_context(&mut self) {
        self.buf.insts.push(BlockInst::SaveContext {
            thread_regs_addr_reg: self.thread_regs_addr_reg,
            tmp_guest_cpsr_reg: self.tmp_guest_cpsr_reg,
            regs_to_save: [None; Reg::SPSR as usize],
        });
    }

    pub fn call(&mut self, func: *const ()) {
        self.call_internal(func, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>)
    }

    pub fn call1(&mut self, func: *const (), arg0: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>)
    }

    pub fn call2(&mut self, func: *const (), arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), None::<BlockOperand>, None::<BlockOperand>)
    }

    pub fn call3(&mut self, func: *const (), arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), None::<BlockOperand>)
    }

    pub fn call4(&mut self, func: *const (), arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>, arg3: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), Some(arg3.into()))
    }

    fn call_internal(
        &mut self,
        func: *const (),
        arg0: Option<impl Into<BlockOperand>>,
        arg1: Option<impl Into<BlockOperand>>,
        arg2: Option<impl Into<BlockOperand>>,
        arg3: Option<impl Into<BlockOperand>>,
    ) {
        let func_reg = self.new_reg();
        self.mov(func_reg, func as u32);

        let mut args = [arg0.map(|arg| arg.into()), arg1.map(|arg| arg.into()), arg2.map(|arg| arg.into()), arg3.map(|arg| arg.into())];
        for (i, arg) in args.iter_mut().enumerate() {
            if let Some(arg) = arg {
                let reg = BlockReg::Fixed(Reg::from(i as u8));
                self.mov(reg, *arg);
                *arg = reg.into();
            }
        }
        self.buf.insts.push(BlockInst::Call {
            func_reg,
            args: [
                args[0].map(|_| BlockReg::Fixed(Reg::R0)),
                args[1].map(|_| BlockReg::Fixed(Reg::R1)),
                args[2].map(|_| BlockReg::Fixed(Reg::R2)),
                args[3].map(|_| BlockReg::Fixed(Reg::R3)),
            ],
        })
    }

    fn resolve_guest_regs_in_basic_blocks(
        &mut self,
        mut guest_regs_written_to: RegReserve,
        guest_regs_mapping: &[Option<BlockReg>; Reg::SPSR as usize],
        basic_blocks: &mut [BasicBlock],
        enter_block: Option<usize>,
        indices: &[usize],
    ) {
        for i in indices {
            let exit_blocks = {
                let basic_block = &mut basic_blocks[*i];
                if let Some(enter_block) = enter_block {
                    basic_block.enter_blocks.insert(enter_block);
                }
                basic_block.init_resolve_guest_regs(self, &mut guest_regs_written_to, guest_regs_mapping);
                basic_block.exit_blocks.clone()
            };
            self.resolve_guest_regs_in_basic_blocks(guest_regs_written_to, guest_regs_mapping, basic_blocks, Some(*i), &exit_blocks);
        }
    }

    fn resolve_io_in_basic_blocks(required_outputs: BlockRegSet, basic_blocks: &mut [BasicBlock], indices: &[usize]) {
        for i in indices {
            let (required_inputs, enter_blocks) = {
                let basic_block = &mut basic_blocks[*i];
                basic_block.init_io(required_outputs);
                (basic_block.get_required_inputs(), Vec::from_iter(basic_block.enter_blocks.clone()))
            };
            Self::resolve_io_in_basic_blocks(required_inputs, basic_blocks, &enter_blocks)
        }
    }

    fn assemble_basic_blocks(&mut self, pc: u32) -> Vec<BasicBlock> {
        let mut basic_blocks = Vec::new();
        // Always emit the first instruction (which is defining the ptr to thread regs as a constant)
        // Manually push this instruction to the first block later after initializing live ranges of variables
        let mut basic_block_start = 1;
        let mut basic_block_label_mapping = NoHashMap::<u16, usize>::default();
        let mut guest_regs = RegReserve::new();
        for i in 0..self.buf.insts.len() {
            let inst = unsafe { self.buf.insts.get_unchecked(i) };
            match inst {
                BlockInst::Label(label) => {
                    basic_block_label_mapping.insert(label.0, basic_blocks.len());
                    if basic_block_start < i {
                        basic_blocks.push(BasicBlock::new(basic_block_start, i - 1));
                        basic_block_start = i;
                    }
                }
                BlockInst::Branch { .. } => {
                    if basic_block_start <= i {
                        basic_blocks.push(BasicBlock::new(basic_block_start, i));
                        basic_block_start = i + 1;
                    }
                }
                _ => {}
            }
            let (inputs, outputs) = inst.get_io();
            guest_regs += inputs.get_guests() + outputs.get_guests();
        }

        if basic_block_start < self.buf.insts.len() {
            if let BlockInst::Label(label) = self.buf.insts[basic_block_start] {
                basic_block_label_mapping.insert(label.0, basic_blocks.len());
            }
            basic_blocks.push(BasicBlock::new(basic_block_start, self.buf.insts.len() - 1));
        }

        let mut guest_regs_mapping = [None; Reg::SPSR as usize];
        for reg in guest_regs {
            let mapping = &mut guest_regs_mapping[reg as usize];
            if mapping.is_none() {
                *mapping = Some(self.new_reg());
            }
        }

        let basic_blocks_len = basic_blocks.len();
        for (i, basic_block) in basic_blocks.iter_mut().enumerate() {
            if let BlockInst::Branch { label, cond, block_index } = &mut self.buf.insts[basic_block.end_asm_inst] {
                let labelled_block_index = basic_block_label_mapping.get(&label.0).unwrap();
                basic_block.exit_blocks.push(*labelled_block_index);
                *block_index = *labelled_block_index;
                if *cond != Cond::AL && i + 1 < basic_blocks_len {
                    basic_block.exit_blocks.push(i + 1);
                }
            } else if i + 1 < basic_blocks_len {
                basic_block.exit_blocks.push(i + 1);
            }
        }

        self.resolve_guest_regs_in_basic_blocks(RegReserve::new(), &guest_regs_mapping, &mut basic_blocks, None, &[0]);

        // First block should contain thread reg addr start constant
        basic_blocks.first_mut().unwrap().insts.insert(0, self.buf.insts[0]);

        for basic_block in &mut basic_blocks {
            basic_block.init_insts(self, &guest_regs_mapping, pc);
        }

        let basic_blocks_len = basic_blocks.len();
        Self::resolve_io_in_basic_blocks(BlockRegSet::new(), &mut basic_blocks, &[basic_blocks_len - 1]);

        basic_blocks
    }

    pub fn finalize(mut self, pc: u32) -> Vec<u32> {
        let mut reg_allocator = BlockRegAllocator::new();

        let mut basic_blocks = self.assemble_basic_blocks(pc);

        // Extend reg live ranges over all blocks for reg allocation
        for i in (1..basic_blocks.len()).rev() {
            let required_inputs = basic_blocks[i].get_required_inputs();
            for j in (1..i).rev() {
                for live_range in &mut basic_blocks[j].regs_live_ranges {
                    *live_range += required_inputs;
                }
            }
        }

        let mut opcodes = Vec::new();
        let mut branch_placeholders = Vec::new();
        let mut opcodes_offset = Vec::with_capacity(basic_blocks.len());
        for basic_block in basic_blocks {
            opcodes_offset.push(opcodes.len());
            opcodes.extend(basic_block.emit_opcodes(&mut reg_allocator, &mut branch_placeholders, opcodes.len()));
        }

        for branch_placeholder in branch_placeholders {
            let opcode = opcodes[branch_placeholder];
            let cond = Cond::from((opcode >> 28) as u8);
            let block_index = opcode & 0xFFFFFFF;
            let branch_to = opcodes_offset[block_index as usize];
            let diff = branch_to as i32 - branch_placeholder as i32;
            opcodes[branch_placeholder] = B::b(diff - 2, cond);
        }

        opcodes
    }
}

use crate::jit::assembler::basic_block::BasicBlock;
use crate::jit::assembler::block_inst::BlockInst;
use crate::jit::assembler::block_reg_allocator::BlockRegAllocator;
use crate::jit::assembler::block_reg_set::BLOCK_REG_SET_ARRAY_SIZE;
use crate::jit::inst_info::{Shift, ShiftValue};
use crate::jit::reg::Reg;
use crate::jit::ShiftType;
use crate::utils::NoHashMap;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};

pub mod arm;
mod basic_block;
pub mod block_asm;
mod block_inst;
mod block_reg_allocator;
mod block_reg_set;

pub const ANY_REG_LIMIT: u16 = BLOCK_REG_SET_ARRAY_SIZE as u16 * 32 - Reg::None as u16;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BlockReg {
    Any(u16),
    Fixed(Reg),
}

impl Default for BlockReg {
    fn default() -> Self {
        BlockReg::Any(0)
    }
}

impl Hash for BlockReg {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u16(self.get_id())
    }
}

impl From<Reg> for BlockReg {
    fn from(value: Reg) -> Self {
        BlockReg::Any(value as u16)
    }
}

impl TryFrom<BlockOperand> for BlockReg {
    type Error = ();

    fn try_from(value: BlockOperand) -> Result<Self, Self::Error> {
        match value {
            BlockOperand::Reg(reg) => Ok(reg),
            _ => Err(()),
        }
    }
}

impl BlockReg {
    pub const fn get_id(self) -> u16 {
        match self {
            BlockReg::Any(id) => id + Reg::None as u16,
            BlockReg::Fixed(reg) => reg as u16,
        }
    }

    pub fn try_as_fixed(self) -> Option<Reg> {
        match self {
            BlockReg::Fixed(reg) => Some(reg),
            _ => None,
        }
    }

    pub fn as_fixed(self) -> Reg {
        self.try_as_fixed().unwrap()
    }

    pub fn try_as_any(self) -> Option<u16> {
        match self {
            BlockReg::Any(reg) => Some(reg),
            _ => None,
        }
    }

    pub fn as_any(self) -> u16 {
        self.try_as_any().unwrap()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum BlockOperand {
    Reg(BlockReg),
    Imm(u32),
}

impl Debug for BlockOperand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockOperand::Reg(reg) => {
                write!(f, "{reg:?}")
            }
            BlockOperand::Imm(imm) => {
                write!(f, "{:x}", *imm)
            }
        }
    }
}

impl Default for BlockOperand {
    fn default() -> Self {
        BlockOperand::Imm(0)
    }
}

impl From<BlockReg> for BlockOperand {
    fn from(value: BlockReg) -> Self {
        BlockOperand::Reg(value)
    }
}

impl From<u32> for BlockOperand {
    fn from(value: u32) -> Self {
        BlockOperand::Imm(value)
    }
}

impl From<Reg> for BlockOperand {
    fn from(value: Reg) -> Self {
        BlockOperand::Reg(value.into())
    }
}

impl From<*const ()> for BlockOperand {
    fn from(value: *const ()) -> Self {
        (value as u32).into()
    }
}

impl Into<BlockOperand> for ShiftValue {
    fn into(self) -> BlockOperand {
        match self {
            ShiftValue::Reg(reg) => BlockOperand::Reg(reg.into()),
            ShiftValue::Imm(imm) => BlockOperand::Imm(imm as u32),
        }
    }
}

impl BlockOperand {
    pub fn needs_reg_for_imm(&self, max_bits: u32) -> bool {
        match self {
            BlockOperand::Reg(_) => false,
            BlockOperand::Imm(imm) => ((*imm) & !max_bits) != 0,
        }
    }

    pub fn try_as_reg(&self) -> Option<BlockReg> {
        match self {
            BlockOperand::Reg(reg) => Some(*reg),
            BlockOperand::Imm(_) => None,
        }
    }

    pub fn as_reg(&self) -> BlockReg {
        self.try_as_reg().unwrap()
    }

    pub fn try_as_imm(&self) -> Option<u32> {
        match self {
            BlockOperand::Reg(_) => None,
            BlockOperand::Imm(imm) => Some(*imm),
        }
    }

    pub fn as_imm(&self) -> u32 {
        self.try_as_imm().unwrap()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct BlockShift {
    pub shift_type: ShiftType,
    pub value: BlockOperand,
}

impl BlockShift {
    pub fn new(shift_type: ShiftType, value: impl Into<BlockOperand>) -> Self {
        BlockShift { shift_type, value: value.into() }
    }

    pub fn is_none(&self) -> bool {
        match self.value {
            BlockOperand::Reg(_) => false,
            BlockOperand::Imm(imm) => imm == 0,
        }
    }
}

impl Default for BlockShift {
    fn default() -> Self {
        BlockShift {
            shift_type: ShiftType::Lsl,
            value: BlockOperand::Imm(0),
        }
    }
}

impl Into<BlockShift> for Shift {
    fn into(self) -> BlockShift {
        match self {
            Shift::Lsl(v) => BlockShift::new(ShiftType::Lsl, v),
            Shift::Lsr(v) => BlockShift::new(ShiftType::Lsr, v),
            Shift::Asr(v) => BlockShift::new(ShiftType::Asr, v),
            Shift::Ror(v) => BlockShift::new(ShiftType::Ror, v),
        }
    }
}

impl Into<BlockShift> for Option<Shift> {
    fn into(self) -> BlockShift {
        self.map_or_else(|| BlockShift::default(), |shift| shift.into())
    }
}

impl Debug for BlockShift {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_none() {
            write!(f, "None")
        } else {
            write!(f, "{:?} {:?}", self.shift_type, self.value)
        }
    }
}

#[derive(Copy, Clone, Default)]
pub struct BlockOperandShift {
    pub operand: BlockOperand,
    pub shift: BlockShift,
}

impl Debug for BlockOperandShift {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.shift.is_none() {
            write!(f, "{:?}", self.operand)
        } else {
            write!(f, "{:?} {:?}", self.operand, self.shift)
        }
    }
}

impl From<BlockReg> for BlockOperandShift {
    fn from(value: BlockReg) -> Self {
        BlockOperandShift {
            operand: value.into(),
            shift: BlockShift::default(),
        }
    }
}

impl From<BlockOperand> for BlockOperandShift {
    fn from(value: BlockOperand) -> Self {
        BlockOperandShift {
            operand: value,
            shift: BlockShift::default(),
        }
    }
}

impl<OP: Into<BlockOperand>> From<(OP, ShiftType, OP)> for BlockOperandShift {
    fn from(value: (OP, ShiftType, OP)) -> Self {
        BlockOperandShift {
            operand: value.0.into(),
            shift: BlockShift::new(value.1, value.2),
        }
    }
}

impl Into<BlockOperandShift> for Reg {
    fn into(self) -> BlockOperandShift {
        BlockOperandShift {
            operand: self.into(),
            shift: BlockShift::default(),
        }
    }
}

impl Into<BlockOperandShift> for u32 {
    fn into(self) -> BlockOperandShift {
        BlockOperandShift {
            operand: self.into(),
            shift: BlockShift::default(),
        }
    }
}

impl<R: Into<BlockOperand>, S: Into<BlockShift>> From<(R, S)> for BlockOperandShift {
    fn from(value: (R, S)) -> Self {
        BlockOperandShift {
            operand: value.0.into(),
            shift: value.1.into(),
        }
    }
}

impl BlockOperandShift {
    pub fn try_as_reg(&self) -> Option<BlockReg> {
        match self.operand {
            BlockOperand::Reg(reg) => Some(reg),
            _ => None,
        }
    }

    pub fn try_as_reg_mut(&mut self) -> Option<&mut BlockReg> {
        match &mut self.operand {
            BlockOperand::Reg(reg) => Some(reg),
            _ => None,
        }
    }

    pub fn as_reg(&self) -> BlockReg {
        self.try_as_reg().unwrap()
    }

    pub fn try_as_shift_reg(&self) -> Option<BlockReg> {
        self.shift.value.try_into().ok()
    }

    pub fn try_as_shift_reg_mut(&mut self) -> Option<&mut BlockReg> {
        match &mut self.shift.value {
            BlockOperand::Reg(reg) => Some(reg),
            _ => None,
        }
    }

    pub fn as_shift_imm(&self) -> u32 {
        self.try_as_shift_imm().unwrap()
    }

    pub fn try_as_shift_imm(&self) -> Option<u32> {
        match self.shift.value {
            BlockOperand::Imm(imm) => Some(imm),
            _ => None,
        }
    }

    pub fn replace_regs(&mut self, old: BlockReg, new: BlockReg) {
        if let BlockOperand::Reg(reg) = &mut self.operand {
            if *reg == old {
                *reg = new;
            }
        }
        if let Some(reg) = self.try_as_shift_reg_mut() {
            if *reg == old {
                *reg = new;
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct BlockLabel(u16);

pub struct BasicBlocksCache {
    pub basic_blocks: Vec<BasicBlock>,
    pub basic_blocks_unlikely: Vec<BasicBlock>,
}

impl BasicBlocksCache {
    pub fn new() -> Self {
        BasicBlocksCache {
            basic_blocks: Vec::new(),
            basic_blocks_unlikely: Vec::new(),
        }
    }
}

pub struct BlockAsmBuf {
    pub insts: Vec<BlockInst>,
    pub basic_block_label_mapping: NoHashMap<u16, usize>,
    pub guest_branches_mapping: NoHashMap<u32, BlockLabel>,
    pub reg_allocator: BlockRegAllocator,
    pub block_opcode_offsets: Vec<usize>,
    pub opcodes: Vec<u32>,
    pub branch_placeholders: Vec<Vec<usize>>,
}

impl BlockAsmBuf {
    pub fn new() -> Self {
        BlockAsmBuf {
            insts: Vec::new(),
            basic_block_label_mapping: NoHashMap::default(),
            guest_branches_mapping: NoHashMap::default(),
            reg_allocator: BlockRegAllocator::new(),
            block_opcode_offsets: Vec::new(),
            opcodes: Vec::new(),
            branch_placeholders: Vec::new(),
        }
    }
}

use crate::core::thread_regs::ThreadRegs;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::block_inst::BlockInst;
use crate::jit::inst_info::{Shift, ShiftValue};
use crate::jit::reg::Reg;
use crate::jit::ShiftType;
use std::fmt::{Debug, Formatter};

pub mod arm;
mod basic_block;
pub mod block_asm;
mod block_inst;
mod block_reg_allocator;
mod block_reg_set;

pub const ANY_REG_LIMIT: u8 = 64 - Reg::SPSR as u8 * 2;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BlockReg {
    Any(u8),
    Guest(Reg),
    Fixed(Reg),
}

impl Into<BlockReg> for Reg {
    fn into(self) -> BlockReg {
        BlockReg::Guest(self)
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
    pub fn try_as_fixed(self) -> Option<Reg> {
        match self {
            BlockReg::Fixed(reg) => Some(reg),
            _ => None,
        }
    }

    pub fn as_fixed(self) -> Reg {
        self.try_as_fixed().unwrap()
    }
}

#[derive(Copy, Clone)]
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

impl From<BlockReg> for BlockOperand {
    fn from(value: BlockReg) -> Self {
        BlockOperand::Reg(value)
    }
}

impl Into<BlockOperand> for u32 {
    fn into(self) -> BlockOperand {
        BlockOperand::Imm(self)
    }
}

impl Into<BlockOperand> for Reg {
    fn into(self) -> BlockOperand {
        BlockOperand::Reg(self.into())
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
}

#[derive(Copy, Clone)]
pub struct BlockShift {
    pub shift_type: ShiftType,
    pub value: BlockOperand,
}

impl BlockShift {
    pub fn new(shift_type: ShiftType, value: impl Into<BlockOperand>) -> Self {
        BlockShift { shift_type, value: value.into() }
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
        match self.value {
            BlockOperand::Reg(reg) => {
                write!(f, "{:?} {reg:?}", self.shift_type)
            }
            BlockOperand::Imm(imm) => {
                if imm == 0 {
                    write!(f, "None")
                } else {
                    write!(f, "{:?} {imm}", self.shift_type)
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct BlockOperandShift {
    pub operand: BlockOperand,
    pub shift: BlockShift,
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

pub struct BlockAsmBuf {
    pub insts: Vec<BlockInst>,
}

impl BlockAsmBuf {
    pub fn new() -> Self {
        BlockAsmBuf { insts: Vec::new() }
    }

    pub fn new_asm(&mut self, thread_regs: &ThreadRegs) -> BlockAsm {
        BlockAsm::new(thread_regs, self)
    }
}

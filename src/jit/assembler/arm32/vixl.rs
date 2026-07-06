use crate::jit;
use crate::jit::reg::Reg;
use std::ops::{Deref, DerefMut};
use vixl::*;

pub struct OperandWrapper(Operand);

impl Deref for OperandWrapper {
    type Target = Operand;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OperandWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<(Reg, jit::ShiftType, Reg)> for OperandWrapper {
    fn from((reg, shift_type, shift_reg): (Reg, jit::ShiftType, Reg)) -> Self {
        OperandWrapper(unsafe { Operand::new5(reg.into(), Shift { shift_: shift_type as _ }, shift_reg.into()) })
    }
}

impl From<(Reg, jit::ShiftType, u8)> for OperandWrapper {
    fn from((reg, shift_type, shift_imm): (Reg, jit::ShiftType, u8)) -> Self {
        OperandWrapper(unsafe { Operand::new4(reg.into(), Shift { shift_: shift_type as _ }, shift_imm as _) })
    }
}

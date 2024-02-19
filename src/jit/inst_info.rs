use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg, AluShiftImm, MulReg};
use crate::jit::inst_info_thumb::InstInfoThumb;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, Op, ShiftType};
use bilge::prelude::{u2, u4, u5};

#[derive(Clone, Debug)]
pub struct InstInfo {
    pub opcode: u32,
    pub op: Op,
    pub cond: Cond,
    operands: Operands,
    pub src_regs: RegReserve,
    pub out_regs: RegReserve,
    pub cycle: u8,
}

impl InstInfo {
    pub fn new(
        opcode: u32,
        op: Op,
        operands: Operands,
        src_regs: RegReserve,
        out_regs: RegReserve,
        cycle: u8,
    ) -> Self {
        InstInfo {
            opcode,
            op,
            cond: Cond::from((opcode >> 28) as u8),
            operands,
            src_regs,
            out_regs,
            cycle,
        }
    }

    pub fn operands(&self) -> &[Operand] {
        &self.operands.values[..self.operands.num as usize]
    }

    pub fn operands_mut(&mut self) -> &mut [Operand] {
        &mut self.operands.values[..self.operands.num as usize]
    }

    pub fn set_cond(&mut self, cond: Cond) {
        self.cond = cond;
        self.opcode = (self.opcode & !(0xF << 28)) | ((cond as u32) << 28);
    }

    pub fn assemble(self) -> u32 {
        let operands = self.operands();
        match self.op {
            Op::AddImm
            | Op::AndImm
            | Op::AndsImm
            | Op::BicImm
            | Op::RscsImm
            | Op::SubImm
            | Op::SubsImm => {
                let mut opcode = AluImm::from(self.opcode);
                let reg0 = operands[0].as_reg_no_shift().unwrap();
                let reg1 = operands[1].as_reg_no_shift().unwrap();
                opcode.set_rd(u4::new(*reg0 as u8));
                opcode.set_rn(u4::new(*reg1 as u8));

                u32::from(opcode)
            }
            Op::MovImm | Op::MvnImm | Op::MvnsImm => {
                let mut opcode = AluImm::from(self.opcode);
                let reg0 = operands[0].as_reg_no_shift().unwrap();
                opcode.set_rd(u4::new(*reg0 as u8));

                u32::from(opcode)
            }
            Op::CmpImm => {
                let mut opcode = AluImm::from(self.opcode);
                let reg0 = operands[0].as_reg_no_shift().unwrap();
                opcode.set_rn(u4::new(*reg0 as u8));

                u32::from(opcode)
            }
            Op::CmpLli => {
                let mut opcode = AluShiftImm::from(self.opcode);
                let reg1 = operands[0].as_reg_no_shift().unwrap();
                let (reg2, shift_2) = operands[1].as_reg().unwrap();
                opcode.set_rm(u4::new(*reg2 as u8));
                opcode.set_rn(u4::new(*reg1 as u8));
                match shift_2 {
                    Some(shift) => {
                        let (shift_type, value) = (*shift).into();
                        opcode.set_shift_type(u2::new(shift_type as u8));
                        opcode.set_shift_imm(u5::new(value.as_imm().unwrap()))
                    }
                    None => opcode.set_shift_imm(u5::new(0)),
                }

                u32::from(opcode)
            }
            Op::AddLli
            | Op::AndLli
            | Op::AndRri
            | Op::EorLli
            | Op::MovLli
            | Op::MovLri
            | Op::MovRri
            | Op::MovsLli
            | Op::MovsRri
            | Op::MvnLli
            | Op::OrrLli
            | Op::RscsLli
            | Op::RscsLri
            | Op::SubLli
            | Op::SubsLli => {
                let mut opcode = AluShiftImm::from(self.opcode);
                let reg0 = operands[0].as_reg_no_shift().unwrap();
                let (reg1, (reg2, shift_2)) = if operands.len() == 3 {
                    (operands[1].as_reg_no_shift(), operands[2].as_reg().unwrap())
                } else {
                    (None, operands[1].as_reg().unwrap())
                };
                opcode.set_rm(u4::new(*reg2 as u8));
                if let Some(reg1) = reg1 {
                    opcode.set_rn(u4::new(*reg1 as u8));
                }
                opcode.set_rd(u4::new(*reg0 as u8));
                match shift_2 {
                    Some(shift) => {
                        let (shift_type, value) = (*shift).into();
                        opcode.set_shift_type(u2::new(shift_type as u8));
                        opcode.set_shift_imm(u5::new(value.as_imm().unwrap()))
                    }
                    None => opcode.set_shift_imm(u5::new(0)),
                }

                u32::from(opcode)
            }
            Op::MovsLlr => {
                let mut opcode = AluReg::from(self.opcode);
                let reg0 = operands[0].as_reg_no_shift().unwrap();
                let (reg1, (reg2, shift_2)) = if operands.len() == 3 {
                    (operands[1].as_reg_no_shift(), operands[2].as_reg().unwrap())
                } else {
                    (None, operands[1].as_reg().unwrap())
                };
                opcode.set_rm(u4::new(*reg2 as u8));
                if let Some(reg1) = reg1 {
                    opcode.set_rn(u4::new(*reg1 as u8));
                }
                opcode.set_rd(u4::new(*reg0 as u8));
                if let Some(shift) = shift_2 {
                    let (shift_type, value) = (*shift).into();
                    opcode.set_shift_type(u2::new(shift_type as u8));
                    opcode.set_rs(u4::new(value.as_reg().unwrap() as u8))
                }

                u32::from(opcode)
            }
            Op::Mul | Op::Mla => {
                let mut opcode = MulReg::from(self.opcode);
                let reg0 = *operands[0].as_reg_no_shift().unwrap();
                let reg1 = *operands[1].as_reg_no_shift().unwrap();
                let reg2 = *operands[2].as_reg_no_shift().unwrap();
                opcode.set_rd(u4::new(reg0 as u8));
                opcode.set_rm(u4::new(reg1 as u8));
                opcode.set_rs(u4::new(reg2 as u8));

                if operands.len() == 4 {
                    let reg3 = *operands[3].as_reg_no_shift().unwrap();
                    opcode.set_rn(u4::new(reg3 as u8));
                }

                u32::from(opcode)
            }
            _ => todo!("{:?}", self),
        }
    }
}

impl From<&InstInfoThumb> for InstInfo {
    fn from(value: &InstInfoThumb) -> Self {
        InstInfo {
            opcode: value.opcode as u32,
            op: value.op,
            cond: Cond::AL,
            operands: value.operands,
            src_regs: value.src_regs,
            out_regs: value.out_regs,
            cycle: value.cycle,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Operands {
    values: [Operand; 4],
    num: u8,
}

impl Operands {
    pub fn new_empty() -> Self {
        Operands {
            values: [Operand::None; 4],
            num: 0,
        }
    }

    pub fn new_1(operand: Operand) -> Self {
        Operands {
            values: [operand, Operand::None, Operand::None, Operand::None],
            num: 1,
        }
    }

    pub fn new_2(operand1: Operand, operand2: Operand) -> Self {
        Operands {
            values: [operand1, operand2, Operand::None, Operand::None],
            num: 2,
        }
    }

    pub fn new_3(operand1: Operand, operand2: Operand, operand3: Operand) -> Self {
        Operands {
            values: [operand1, operand2, operand3, Operand::None],
            num: 3,
        }
    }

    pub fn new_4(
        operand1: Operand,
        operand2: Operand,
        operand3: Operand,
        operand4: Operand,
    ) -> Self {
        Operands {
            values: [operand1, operand2, operand3, operand4],
            num: 4,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Operand {
    Reg { reg: Reg, shift: Option<Shift> },
    Imm(u32),
    None,
}

impl Operand {
    pub fn reg(reg: Reg) -> Self {
        Operand::Reg { reg, shift: None }
    }

    pub fn reg_imm_shift(reg: Reg, shift_type: ShiftType, imm: u8) -> Self {
        let shift_value = ShiftValue::Imm(imm);
        Operand::Reg {
            reg,
            shift: Some(match shift_type {
                ShiftType::Lsl => Shift::Lsl(shift_value),
                ShiftType::Lsr => Shift::Lsr(shift_value),
                ShiftType::Asr => Shift::Asr(shift_value),
                ShiftType::Ror => Shift::Ror(shift_value),
            }),
        }
    }

    pub fn reg_reg_shift(reg: Reg, shift_type: ShiftType, shift: Reg) -> Self {
        let shift_value = ShiftValue::Reg(shift);
        Operand::Reg {
            reg,
            shift: Some(match shift_type {
                ShiftType::Lsl => Shift::Lsl(shift_value),
                ShiftType::Lsr => Shift::Lsr(shift_value),
                ShiftType::Asr => Shift::Asr(shift_value),
                ShiftType::Ror => Shift::Ror(shift_value),
            }),
        }
    }

    pub fn imm(imm: u32) -> Self {
        Operand::Imm(imm)
    }

    pub fn as_reg(&self) -> Option<(&Reg, &Option<Shift>)> {
        match self {
            Operand::Reg { reg, shift } => Some((reg, shift)),
            _ => None,
        }
    }

    pub fn as_reg_no_shift(&self) -> Option<&Reg> {
        let (reg, shift) = self.as_reg().unwrap();
        match shift {
            None => Some(reg),
            Some(_) => None,
        }
    }

    pub fn as_imm(&self) -> Option<&u32> {
        match self {
            Operand::Imm(imm) => Some(imm),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ShiftValue {
    Reg(Reg),
    Imm(u8),
}

impl ShiftValue {
    pub fn as_imm(&self) -> Option<u8> {
        match self {
            ShiftValue::Imm(imm) => Some(*imm),
            _ => None,
        }
    }

    pub fn as_reg(&self) -> Option<Reg> {
        match self {
            ShiftValue::Reg(reg) => Some(*reg),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Shift {
    Lsl(ShiftValue),
    Lsr(ShiftValue),
    Asr(ShiftValue),
    Ror(ShiftValue),
}

impl From<Shift> for (ShiftType, ShiftValue) {
    fn from(value: Shift) -> Self {
        match value {
            Shift::Lsl(v) => (ShiftType::Lsl, v),
            Shift::Lsr(v) => (ShiftType::Lsr, v),
            Shift::Asr(v) => (ShiftType::Asr, v),
            Shift::Ror(v) => (ShiftType::Ror, v),
        }
    }
}

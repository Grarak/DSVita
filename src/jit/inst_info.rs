use crate::jit::inst_info_thumb::InstInfoThumb;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, Op, ShiftType};
use std::fmt::{Debug, Formatter};

#[derive(Clone)]
pub struct InstInfo {
    pub opcode: u32,
    pub op: Op,
    pub cond: Cond,
    pub operands: Operands,
    pub src_regs: RegReserve,
    pub out_regs: RegReserve,
    pub cycle: u8,
}

impl InstInfo {
    pub fn new(opcode: u32, op: Op, operands: Operands, mut src_regs: RegReserve, out_regs: RegReserve, cycle: u8) -> Self {
        let cond = Cond::from((opcode >> 28) as u8);
        if cond != Cond::AL {
            src_regs += Reg::CPSR;
        }
        InstInfo {
            opcode,
            op,
            cond,
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

    pub fn is_branch(&self) -> bool {
        self.out_regs.is_reserved(Reg::PC) || self.op.is_branch()
    }

    pub fn is_uncond_branch(&self) -> bool {
        self.cond == Cond::AL && self.out_regs.is_reserved(Reg::PC)
    }

    fn get_branch_cond(&self) -> Cond {
        debug_assert!(self.op.is_branch());
        match self.op {
            Op::BeqT => Cond::EQ,
            Op::BneT => Cond::NE,
            Op::BcsT => Cond::HS,
            Op::BccT => Cond::LO,
            Op::BmiT => Cond::MI,
            Op::BplT => Cond::PL,
            Op::BvsT => Cond::VS,
            Op::BvcT => Cond::VC,
            Op::BhiT => Cond::HI,
            Op::BlsT => Cond::LS,
            Op::BgeT => Cond::GE,
            Op::BltT => Cond::LT,
            Op::BgtT => Cond::GT,
            Op::BleT => Cond::LE,
            _ => self.cond,
        }
    }

    pub fn imm_transfer_addr(&self, pc: u32) -> Option<u32> {
        match self.op {
            Op::Ldr(transfer) | Op::Str(transfer) | Op::LdrT(transfer) | Op::StrT(transfer)
                if !transfer.write_back() && {
                    let operands = self.operands();
                    operands[1].as_reg_no_shift() == Some(Reg::PC) && operands[2].as_imm().is_some()
                } =>
            {
                let thumb = matches!(self.op, Op::LdrT(_) | Op::StrT(_));
                let pc = pc + if thumb { 4 } else { 8 };
                let offset = self.operands()[2].as_imm().unwrap();
                let addr = if transfer.add() { pc + offset } else { pc - offset };
                Some(if thumb { addr & !0x3 } else { addr })
            }
            _ => None,
        }
    }
}

impl Debug for InstInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "InstInfo {{ {:x}, {:?}, {:?} {:?}, src: {:?}, out: {:?}, cycles: {} }}",
            self.opcode, self.cond, self.op, self.operands, self.src_regs, self.out_regs, self.cycle
        )
    }
}

impl From<InstInfoThumb> for InstInfo {
    fn from(value: InstInfoThumb) -> Self {
        let mut info = InstInfo {
            opcode: value.opcode as u32,
            op: value.op,
            cond: Cond::AL,
            operands: value.operands,
            src_regs: value.src_regs,
            out_regs: value.out_regs,
            cycle: value.cycle,
        };
        if info.op.is_branch() {
            info.cond = info.get_branch_cond();
        }
        info
    }
}

#[derive(Copy, Clone)]
pub struct Operands {
    pub values: [Operand; 4],
    num: u8,
}

impl Operands {
    pub fn new_empty() -> Self {
        Operands { values: [Operand::None; 4], num: 0 }
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

    pub fn new_4(operand1: Operand, operand2: Operand, operand3: Operand, operand4: Operand) -> Self {
        Operands {
            values: [operand1, operand2, operand3, operand4],
            num: 4,
        }
    }
}

impl Debug for Operands {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug_list = f.debug_list();
        for i in 0..self.num {
            debug_list.entry(&self.values[i as usize]);
        }
        debug_list.finish()
    }
}

#[derive(Copy, Clone)]
pub enum Operand {
    Reg { reg: Reg, shift: Option<Shift> },
    Imm(u32),
    RegList(RegReserve),
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

    pub fn reg_list(reg: RegReserve) -> Self {
        Operand::RegList(reg)
    }

    pub fn as_reg(&self) -> Option<(Reg, Option<Shift>)> {
        match self {
            Operand::Reg { reg, shift } => Some((*reg, *shift)),
            _ => None,
        }
    }

    pub fn as_reg_no_shift(&self) -> Option<Reg> {
        match self.as_reg() {
            None => None,
            Some((reg, shift)) => match shift {
                None => Some(reg),
                Some(_) => None,
            },
        }
    }

    pub fn as_imm(&self) -> Option<u32> {
        match self {
            Operand::Imm(imm) => Some(*imm),
            _ => None,
        }
    }

    pub fn as_reg_list(&self) -> Option<RegReserve> {
        match self {
            Operand::RegList(list) => Some(*list),
            _ => None,
        }
    }
}

impl Debug for Operand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Operand::Reg { reg, shift } => match shift {
                None => write!(f, "Reg({reg:?})"),
                Some(shift) => write!(f, "Reg {{ {reg:?} {shift:?} }}"),
            },
            Operand::Imm(imm) => write!(f, "Imm({imm:x})"),
            Operand::RegList(reg) => write!(f, "RegList({reg:?})"),
            Operand::None => write!(f, "None"),
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

mod alu_variations {
    use crate::jit::reg::Reg;

    #[inline]
    pub fn imm_shift(opcode: u32) -> (Reg, u8) {
        let reg = Reg::from((opcode & 0xF) as u8);
        let shift = (opcode >> 7) & 0x1F;
        (reg, shift as u8)
    }

    #[inline]
    pub fn reg_shift(opcode: u32) -> (Reg, Reg) {
        let reg = Reg::from((opcode & 0xF) as u8);
        let shift = Reg::from(((opcode >> 8) & 0xF) as u8);
        (reg, shift)
    }

    #[inline]
    pub fn imm(opcode: u32) -> u32 {
        let value = opcode & 0xFF;
        let shift = (opcode >> 7) & 0x1E;
        value.rotate_right(shift)
    }
}

pub(super) use alu_variations::*;

mod alu_ops {
    use crate::jit::inst_info::{InstInfo, Operand, Operands};
    use crate::jit::reg::{reg_reserve, Reg};
    use crate::jit::{Op, ShiftType};

    #[inline]
    pub fn alu3_imm<const CPSR_INPUT: bool, const CPSR_OUTPUT: bool>(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1) + if CPSR_INPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            reg_reserve!(op0) + if CPSR_OUTPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            1,
        )
    }

    #[inline]
    pub fn alu3_imm_shift<const SHIFT_TYPE: ShiftType, const CPSR_INPUT: bool, const CPSR_OUTPUT: bool>(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let ror_with_carry = SHIFT_TYPE == ShiftType::Ror && operand2.1 == 0;
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg_imm_shift(operand2.0, SHIFT_TYPE, operand2.1)),
            reg_reserve!(op1, operand2.0) + if CPSR_INPUT || ror_with_carry { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            reg_reserve!(op0) + if CPSR_OUTPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            1,
        )
    }

    #[inline]
    pub fn alu3_reg_shift<const SHIFT_TYPE: ShiftType, const CPSR_INPUT: bool, const CPSR_OUTPUT: bool>(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg_reg_shift(operand2.0, SHIFT_TYPE, operand2.1)),
            reg_reserve!(op1, operand2.0, operand2.1) + if CPSR_INPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            reg_reserve!(op0) + if CPSR_OUTPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            1,
        )
    }

    #[inline]
    pub fn alu2_op1_imm<const CPSR_INPUT: bool>(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1) + if CPSR_INPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            reg_reserve!(Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn alu2_op1_imm_shift<const SHIFT_TYPE: ShiftType, const CPSR_INPUT: bool>(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let ror_with_carry = SHIFT_TYPE == ShiftType::Ror && operand2.1 == 0;
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op1), Operand::reg_imm_shift(operand2.0, SHIFT_TYPE, operand2.1)),
            reg_reserve!(op1, operand2.0) + if CPSR_INPUT || ror_with_carry { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            reg_reserve!(Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn alu2_op1_reg_shift<const SHIFT_TYPE: ShiftType, const CPSR_INPUT: bool>(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op1), Operand::reg_reg_shift(operand2.0, SHIFT_TYPE, operand2.1)),
            reg_reserve!(op1, operand2.0, operand2.1) + if CPSR_INPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            reg_reserve!(Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn alu2_op0_imm<const CPSR_INPUT: bool, const CPSR_OUTPUT: bool>(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::imm(operand2)),
            if CPSR_INPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            reg_reserve!(op0) + if CPSR_OUTPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            1,
        )
    }

    #[inline]
    pub fn alu2_op0_imm_shift<const SHIFT_TYPE: ShiftType, const CPSR_INPUT: bool, const CPSR_OUTPUT: bool>(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let ror_with_carry = SHIFT_TYPE == ShiftType::Ror && operand2.1 == 0;
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_imm_shift(operand2.0, SHIFT_TYPE, operand2.1)),
            reg_reserve!(operand2.0) + if CPSR_INPUT || ror_with_carry { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            reg_reserve!(op0) + if CPSR_OUTPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            1,
        )
    }

    #[inline]
    pub fn alu2_op0_reg_shift<const SHIFT_TYPE: ShiftType, const CPSR_INPUT: bool, const CPSR_OUTPUT: bool>(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_reg_shift(operand2.0, SHIFT_TYPE, operand2.1)),
            reg_reserve!(operand2.0, operand2.1) + if CPSR_INPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            reg_reserve!(op0) + if CPSR_OUTPUT { reg_reserve!(Reg::CPSR) } else { reg_reserve!() },
            1,
        )
    }

    #[inline]
    pub fn mul(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        let op2 = Reg::from(((opcode >> 8) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2)),
            reg_reserve!(op1, op2),
            reg_reserve!(op0),
            5,
        )
    }

    #[inline]
    pub fn mla(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        let op2 = Reg::from(((opcode >> 8) & 0xF) as u8);
        let op3 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_4(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2), Operand::reg(op3)),
            reg_reserve!(op1, op2, op3),
            reg_reserve!(op0),
            6,
        )
    }

    #[inline]
    pub fn umull(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op2 = Reg::from((opcode & 0xF) as u8);
        let op3 = Reg::from(((opcode >> 8) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_4(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2), Operand::reg(op3)),
            reg_reserve!(op2, op3),
            reg_reserve!(op0, op1),
            6,
        )
    }

    #[inline]
    pub fn umlal(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op2 = Reg::from((opcode & 0xF) as u8);
        let op3 = Reg::from(((opcode >> 8) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_4(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2), Operand::reg(op3)),
            reg_reserve!(op0, op1, op2, op3),
            reg_reserve!(op0, op1),
            7,
        )
    }

    #[inline]
    pub fn smull(opcode: u32, op: Op) -> InstInfo {
        umull(opcode, op)
    }

    #[inline]
    pub fn smlal(opcode: u32, op: Op) -> InstInfo {
        umlal(opcode, op)
    }

    #[inline]
    pub fn muls(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        let op2 = Reg::from(((opcode >> 8) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2)),
            reg_reserve!(op1, op2, Reg::CPSR),
            reg_reserve!(op0, Reg::CPSR),
            5,
        )
    }

    #[inline]
    pub fn mlas(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        let op2 = Reg::from(((opcode >> 8) & 0xF) as u8);
        let op3 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_4(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2), Operand::reg(op3)),
            reg_reserve!(op1, op2, op3, Reg::CPSR),
            reg_reserve!(op0, Reg::CPSR),
            6,
        )
    }

    #[inline]
    pub fn umulls(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op2 = Reg::from((opcode & 0xF) as u8);
        let op3 = Reg::from(((opcode >> 8) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_4(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2), Operand::reg(op3)),
            reg_reserve!(op2, op3, Reg::CPSR),
            reg_reserve!(op0, op1, Reg::CPSR),
            6,
        )
    }

    #[inline]
    pub fn umlals(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op2 = Reg::from((opcode & 0xF) as u8);
        let op3 = Reg::from(((opcode >> 8) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_4(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2), Operand::reg(op3)),
            reg_reserve!(op0, op1, op2, op3, Reg::CPSR),
            reg_reserve!(op0, op1, Reg::CPSR),
            7,
        )
    }

    #[inline]
    pub fn smulls(opcode: u32, op: Op) -> InstInfo {
        umulls(opcode, op)
    }

    #[inline]
    pub fn smlals(opcode: u32, op: Op) -> InstInfo {
        umlals(opcode, op)
    }

    #[inline]
    pub fn smulbb(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        let op2 = Reg::from(((opcode >> 8) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2)),
            reg_reserve!(op1, op2),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn smulbt(opcode: u32, op: Op) -> InstInfo {
        smulbb(opcode, op)
    }

    #[inline]
    pub fn smultb(opcode: u32, op: Op) -> InstInfo {
        smulbb(opcode, op)
    }

    #[inline]
    pub fn smultt(opcode: u32, op: Op) -> InstInfo {
        smulbb(opcode, op)
    }

    #[inline]
    pub fn smulwb(opcode: u32, op: Op) -> InstInfo {
        smulbb(opcode, op)
    }

    #[inline]
    pub fn smulwt(opcode: u32, op: Op) -> InstInfo {
        smulbb(opcode, op)
    }

    #[inline]
    pub fn smlabb(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        let op2 = Reg::from(((opcode >> 8) & 0xF) as u8);
        let op3 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_4(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2), Operand::reg(op3)),
            reg_reserve!(op1, op2, op3, Reg::CPSR),
            reg_reserve!(op0, Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn smlabt(opcode: u32, op: Op) -> InstInfo {
        smlabb(opcode, op)
    }

    #[inline]
    pub fn smlatb(opcode: u32, op: Op) -> InstInfo {
        smlabb(opcode, op)
    }

    #[inline]
    pub fn smlatt(opcode: u32, op: Op) -> InstInfo {
        smlabb(opcode, op)
    }

    #[inline]
    pub fn smlawb(opcode: u32, op: Op) -> InstInfo {
        smlabb(opcode, op)
    }

    #[inline]
    pub fn smlawt(opcode: u32, op: Op) -> InstInfo {
        smlabb(opcode, op)
    }

    #[inline]
    pub fn smlalbb(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let op2 = Reg::from((opcode & 0xF) as u8);
        let op3 = Reg::from(((opcode >> 8) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_4(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2), Operand::reg(op3)),
            reg_reserve!(op0, op1, op2, op3),
            reg_reserve!(op0, op1),
            1,
        )
    }

    #[inline]
    pub fn smlalbt(opcode: u32, op: Op) -> InstInfo {
        smlalbb(opcode, op)
    }

    #[inline]
    pub fn smlaltb(opcode: u32, op: Op) -> InstInfo {
        smlalbb(opcode, op)
    }

    #[inline]
    pub fn smlaltt(opcode: u32, op: Op) -> InstInfo {
        smlalbb(opcode, op)
    }

    #[inline]
    pub fn qadd(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        let op2 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2)),
            reg_reserve!(op1, op2, Reg::CPSR),
            reg_reserve!(op0, Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn qsub(opcode: u32, op: Op) -> InstInfo {
        qadd(opcode, op)
    }

    #[inline]
    pub fn qdadd(opcode: u32, op: Op) -> InstInfo {
        qadd(opcode, op)
    }

    #[inline]
    pub fn qdsub(opcode: u32, op: Op) -> InstInfo {
        qadd(opcode, op)
    }

    #[inline]
    pub fn clz(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(opcode, op, Operands::new_2(Operand::reg(op0), Operand::reg(op1)), reg_reserve!(op1), reg_reserve!(op0), 1)
    }
}

pub(super) use alu_ops::*;

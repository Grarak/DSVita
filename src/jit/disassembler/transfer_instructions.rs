mod transfer_variations {
    use crate::jit::reg::Reg;

    #[inline]
    pub fn imm(opcode: u32) -> u32 {
        opcode & 0xFFF
    }

    #[inline]
    pub fn imm_h(opcode: u32) -> u32 {
        ((opcode >> 4) & 0xF0) | (opcode & 0xF)
    }

    #[inline]
    pub fn reg(opcode: u32) -> Reg {
        Reg::from((opcode & 0xF) as u8)
    }

    #[inline]
    pub fn reg_imm_shift(opcode: u32) -> (Reg, u8) {
        let reg = Reg::from((opcode & 0xF) as u8);
        let shift = ((opcode >> 7) & 0x1F) as u8;
        (reg, shift)
    }
}

pub(super) use transfer_variations::*;

mod transfer_ops {
    use crate::jit::inst_info::{InstInfo, Operand, Operands};
    use crate::jit::reg::{reg_reserve, Reg, RegReserve};
    use crate::jit::{Op, ShiftType};

    #[inline]
    pub fn mem_transfer_imm<const WRITE: bool, const WRITE_BACK: bool, const IS_64BIT: bool>(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let mut src_regs = if WRITE { reg_reserve!(op0, op1) } else { reg_reserve!(op1) };
        let mut out_regs = if WRITE {
            if WRITE_BACK {
                reg_reserve!(op1)
            } else {
                reg_reserve!()
            }
        } else if WRITE_BACK {
            reg_reserve!(op0, op1)
        } else {
            reg_reserve!(op0)
        };

        if IS_64BIT {
            if op0 as u8 & 1 == 1 || op0 > Reg::R12 {
                return InstInfo::new(opcode, Op::UnkArm, Operands::new_empty(), reg_reserve!(), reg_reserve!(), 0);
            }
            if WRITE {
                src_regs += Reg::from(op0 as u8 + 1);
            } else {
                out_regs += Reg::from(op0 as u8 + 1);
            }
        }
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            src_regs,
            out_regs,
            if WRITE { 2 } else { 3 },
        )
    }

    #[inline]
    pub fn mem_transfer_reg<const WRITE: bool, const WRITE_BACK: bool, const IS_64BIT: bool>(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let mut src_regs = if WRITE { reg_reserve!(op0, op1, operand2) } else { reg_reserve!(op1, operand2) };
        let mut out_regs = if WRITE {
            if WRITE_BACK {
                reg_reserve!(op1)
            } else {
                reg_reserve!()
            }
        } else if WRITE_BACK {
            reg_reserve!(op0, op1)
        } else {
            reg_reserve!(op0)
        };

        if IS_64BIT {
            if op0 as u8 & 1 == 1 || op0 > Reg::R12 {
                return InstInfo::new(opcode, Op::UnkArm, Operands::new_empty(), reg_reserve!(), reg_reserve!(), 0);
            }
            if WRITE {
                src_regs += Reg::from(op0 as u8 + 1);
            } else {
                out_regs += Reg::from(op0 as u8 + 1);
            }
        }
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg(operand2)),
            src_regs,
            out_regs,
            if WRITE { 2 } else { 3 },
        )
    }

    #[inline]
    pub fn mem_transfer_reg_shift<const WRITE: bool, const WRITE_BACK: bool, const SHIFT_TYPE: ShiftType, const IS_64BIT: bool>(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let mut src_regs = if WRITE { reg_reserve!(op0, op1, operand2.0) } else { reg_reserve!(op1, operand2.0) };
        let mut out_regs = if WRITE {
            if WRITE_BACK {
                reg_reserve!(op1)
            } else {
                reg_reserve!()
            }
        } else if WRITE_BACK {
            reg_reserve!(op0, op1)
        } else {
            reg_reserve!(op0)
        };

        if IS_64BIT {
            if op0 as u8 & 1 == 1 || op0 > Reg::R12 {
                return InstInfo::new(opcode, Op::UnkArm, Operands::new_empty(), reg_reserve!(), reg_reserve!(), 0);
            }
            if WRITE {
                src_regs += Reg::from(op0 as u8 + 1);
            } else {
                out_regs += Reg::from(op0 as u8 + 1);
            }
        }
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg_imm_shift(operand2.0, SHIFT_TYPE, operand2.1)),
            src_regs,
            out_regs,
            if WRITE { 2 } else { 3 },
        )
    }

    #[inline]
    pub fn swpb(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        let op2 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2)),
            reg_reserve!(op1, op2),
            reg_reserve!(op0),
            4,
        )
    }

    #[inline]
    pub fn swp(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from((opcode & 0xF) as u8);
        let op2 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2)),
            reg_reserve!(op1, op2),
            reg_reserve!(op0),
            4,
        )
    }

    #[inline]
    pub fn ldmda(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            reg_reserve!(op0),
            rlist + if rlist.is_empty() { reg_reserve!(op0) } else { reg_reserve!() },
            rlist.len() as u8 + 2,
        )
    }

    #[inline]
    pub fn stmda(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            rlist + op0,
            if rlist.is_empty() { reg_reserve!(op0) } else { reg_reserve!() },
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn ldmia(opcode: u32, op: Op) -> InstInfo {
        ldmda(opcode, op)
    }

    #[inline]
    pub fn stmia(opcode: u32, op: Op) -> InstInfo {
        stmda(opcode, op)
    }

    #[inline]
    pub fn ldmdb(opcode: u32, op: Op) -> InstInfo {
        ldmda(opcode, op)
    }

    #[inline]
    pub fn stmdb(opcode: u32, op: Op) -> InstInfo {
        stmda(opcode, op)
    }

    #[inline]
    pub fn ldmib(opcode: u32, op: Op) -> InstInfo {
        ldmda(opcode, op)
    }

    #[inline]
    pub fn stmib(opcode: u32, op: Op) -> InstInfo {
        stmda(opcode, op)
    }

    #[inline]
    pub fn ldmda_w(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            reg_reserve!(op0),
            rlist + op0,
            rlist.len() as u8 + 2,
        )
    }

    #[inline]
    pub fn stmda_w(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            rlist + op0,
            reg_reserve!(op0),
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn ldmia_w(opcode: u32, op: Op) -> InstInfo {
        ldmda_w(opcode, op)
    }

    #[inline]
    pub fn stmia_w(opcode: u32, op: Op) -> InstInfo {
        stmda_w(opcode, op)
    }

    #[inline]
    pub fn ldmdb_w(opcode: u32, op: Op) -> InstInfo {
        ldmda_w(opcode, op)
    }

    #[inline]
    pub fn stmdb_w(opcode: u32, op: Op) -> InstInfo {
        stmda_w(opcode, op)
    }

    #[inline]
    pub fn ldmib_w(opcode: u32, op: Op) -> InstInfo {
        ldmda_w(opcode, op)
    }

    #[inline]
    pub fn stmib_w(opcode: u32, op: Op) -> InstInfo {
        stmda_w(opcode, op)
    }

    #[inline]
    pub fn ldmda_u(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            reg_reserve!(op0),
            if rlist.is_reserved(Reg::PC) { rlist } else { reg_reserve!() } + if rlist.is_empty() { reg_reserve!(op0) } else { reg_reserve!() },
            rlist.len() as u8 + 2,
        )
    }

    #[inline]
    pub fn stmda_u(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            rlist + op0,
            if rlist.is_reserved(Reg::PC) { rlist } else { reg_reserve!() } + if rlist.is_empty() { reg_reserve!(op0) } else { reg_reserve!() },
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn ldmia_u(opcode: u32, op: Op) -> InstInfo {
        ldmda_u(opcode, op)
    }

    #[inline]
    pub fn stmia_u(opcode: u32, op: Op) -> InstInfo {
        stmda_u(opcode, op)
    }

    #[inline]
    pub fn ldmdb_u(opcode: u32, op: Op) -> InstInfo {
        ldmda_u(opcode, op)
    }

    #[inline]
    pub fn stmdb_u(opcode: u32, op: Op) -> InstInfo {
        stmda_u(opcode, op)
    }

    #[inline]
    pub fn ldmib_u(opcode: u32, op: Op) -> InstInfo {
        ldmda_u(opcode, op)
    }

    #[inline]
    pub fn stmib_u(opcode: u32, op: Op) -> InstInfo {
        stmda_u(opcode, op)
    }

    #[inline]
    pub fn ldmda_u_w(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            rlist + op0,
            if rlist.is_reserved(Reg::PC) { rlist } else { reg_reserve!() } + op0,
            rlist.len() as u8 + 2,
        )
    }

    #[inline]
    pub fn stmda_u_w(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            rlist + op0,
            reg_reserve!(op0),
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn ldmia_u_w(opcode: u32, op: Op) -> InstInfo {
        ldmda_u_w(opcode, op)
    }

    #[inline]
    pub fn stmia_u_w(opcode: u32, op: Op) -> InstInfo {
        stmda_u_w(opcode, op)
    }

    #[inline]
    pub fn ldmdb_u_w(opcode: u32, op: Op) -> InstInfo {
        ldmda_u_w(opcode, op)
    }

    #[inline]
    pub fn stmdb_u_w(opcode: u32, op: Op) -> InstInfo {
        stmda_u_w(opcode, op)
    }

    #[inline]
    pub fn ldmib_u_w(opcode: u32, op: Op) -> InstInfo {
        ldmda_u_w(opcode, op)
    }

    #[inline]
    pub fn stmib_u_w(opcode: u32, op: Op) -> InstInfo {
        stmda_u_w(opcode, op)
    }

    #[inline]
    pub fn msr_rc(opcode: u32, op: Op) -> InstInfo {
        let op1 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(opcode, op, Operands::new_1(Operand::reg(op1)), reg_reserve!(op1), reg_reserve!(), 1)
    }

    #[inline]
    pub fn msr_rs(opcode: u32, op: Op) -> InstInfo {
        let op1 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(opcode, op, Operands::new_1(Operand::reg(op1)), reg_reserve!(op1), reg_reserve!(), 1)
    }

    #[inline]
    pub fn msr_ic(opcode: u32, op: Op) -> InstInfo {
        let op1 = opcode & 0xFF;
        let shift = (opcode >> 7) & 0x1E;
        let op1 = if shift == 0 { op1 } else { (op1 << (32 - shift)) | (op1 >> shift) };
        let flags = (opcode >> 16) & 0xF;
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::imm(op1)),
            reg_reserve!(),
            if flags & 1 == 1 { reg_reserve!() } else { reg_reserve!() },
            1,
        )
    }

    #[inline]
    pub fn msr_is(opcode: u32, op: Op) -> InstInfo {
        let op1 = opcode & 0xFF;
        let shift = (opcode >> 7) & 0x1E;
        let op1 = if shift == 0 { op1 } else { (op1 << (32 - shift)) | (op1 >> shift) };
        InstInfo::new(opcode, op, Operands::new_1(Operand::imm(op1)), reg_reserve!(), reg_reserve!(), 1)
    }

    #[inline]
    pub fn mrs_rc(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(opcode, op, Operands::new_1(Operand::reg(op0)), reg_reserve!(Reg::CPSR), reg_reserve!(op0), 1)
    }

    #[inline]
    pub fn mrs_rs(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(opcode, op, Operands::new_1(Operand::reg(op0)), reg_reserve!(), reg_reserve!(op0), 1)
    }

    #[inline]
    pub fn mrc(opcode: u32, op: Op) -> InstInfo {
        let op2 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(opcode, op, Operands::new_1(Operand::reg(op2)), reg_reserve!(), reg_reserve!(op2), 1)
    }

    #[inline]
    pub fn mcr(opcode: u32, op: Op) -> InstInfo {
        let op2 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(opcode, op, Operands::new_1(Operand::reg(op2)), reg_reserve!(op2), reg_reserve!(), 1)
    }
}

pub(super) use transfer_ops::*;

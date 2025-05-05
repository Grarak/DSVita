mod branch_ops {
    use crate::jit::inst_info::{InstInfo, Operand, Operands};
    use crate::jit::reg::{reg_reserve, Reg};
    use crate::jit::{Cond, Op};

    #[inline]
    pub fn bx(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(opcode, op, Operands::new_1(Operand::reg(op0)), reg_reserve!(op0), reg_reserve!(Reg::PC), 1)
    }

    #[inline]
    pub fn blx_reg(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(opcode, op, Operands::new_1(Operand::reg(op0)), reg_reserve!(op0), reg_reserve!(Reg::LR, Reg::PC), 1)
    }

    #[inline]
    pub fn b(opcode: u32, op: Op) -> InstInfo {
        let op0 = ((opcode << 8) as i32) >> 6; // * 4 (in steps of 4)
        let inst = InstInfo::new(opcode, op, Operands::new_1(Operand::imm(op0 as u32)), reg_reserve!(), reg_reserve!(Reg::PC), 1);
        // blx label
        if inst.cond == Cond::NV {
            let op0 = (((opcode << 8) as i32) >> 6) | ((opcode & (1 << 24)) >> 23) as i32;
            InstInfo::new(
                (opcode & !(0xF << 28)) | ((Cond::AL as u32) << 28),
                Op::Blx,
                Operands::new_1(Operand::imm(op0 as u32)),
                reg_reserve!(),
                reg_reserve!(Reg::LR, Reg::PC),
                1,
            )
        } else {
            inst
        }
    }

    #[inline]
    pub fn bl(opcode: u32, op: Op) -> InstInfo {
        let op0 = ((opcode << 8) as i32) >> 6; // * 4 (in steps of 4)
        let inst = InstInfo::new(opcode, op, Operands::new_1(Operand::imm(op0 as u32)), reg_reserve!(), reg_reserve!(Reg::LR, Reg::PC), 1);
        // blx label
        if inst.cond == Cond::NV {
            let op0 = (((opcode << 8) as i32) >> 6) | ((opcode & (1 << 24)) >> 23) as i32;
            InstInfo::new(
                (opcode & !(0xF << 28)) | ((Cond::AL as u32) << 28),
                Op::Blx,
                Operands::new_1(Operand::imm(op0 as u32)),
                reg_reserve!(),
                reg_reserve!(Reg::LR, Reg::PC),
                1,
            )
        } else {
            inst
        }
    }

    #[inline]
    pub fn swi(opcode: u32, op: Op) -> InstInfo {
        InstInfo::new(opcode, op, Operands::new_empty(), reg_reserve!(), reg_reserve!(), 3)
    }
}

pub(super) use branch_ops::*;

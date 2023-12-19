mod branch_ops {
    use crate::jit::inst_info::{InstCycle, InstInfo, Operand, Operands};
    use crate::jit::reg::{reg_reserve, Reg};
    use crate::jit::Op;

    #[inline]
    pub fn bx(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            reg_reserve!(op0),
            reg_reserve!(Reg::CPSR),
            InstCycle::common(3),
        )
    }

    #[inline]
    pub fn blx_reg(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            reg_reserve!(op0),
            reg_reserve!(Reg::LR, Reg::CPSR),
            InstCycle::common(3),
        )
    }

    #[inline]
    pub fn b(opcode: u32, op: Op) -> InstInfo {
        let op0 = ((opcode << 8) as i32) >> 6; // * 4 (in steps of 4)
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::imm(op0 as u32)),
            reg_reserve!(),
            reg_reserve!(),
            InstCycle::common(3),
        )
    }

    #[inline]
    pub fn bl(opcode: u32, op: Op) -> InstInfo {
        let op0 = ((opcode << 8) as i32) >> 6; // * 4 (in steps of 4)
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::imm(op0 as u32)),
            reg_reserve!(),
            reg_reserve!(Reg::LR),
            InstCycle::common(3),
        )
    }

    #[inline]
    pub fn swi(opcode: u32, op: Op) -> InstInfo {
        InstInfo::new(
            opcode,
            op,
            Operands::new_empty(),
            reg_reserve!(),
            reg_reserve!(),
            InstCycle::common(3),
        )
    }
}

pub use branch_ops::*;

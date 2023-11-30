mod branch_ops {
    use crate::jit::inst_info::{InstInfo, Operand, Operands};
    use crate::jit::reg::{reg_reserve, Reg};
    use crate::jit::Op;

    #[inline]
    pub fn bx(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn blx_reg(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            reg_reserve!(op0),
            reg_reserve!(),
        )
    }

    #[inline]
    pub fn b(opcode: u32, op: Op) -> InstInfo {
        let op0 = ((opcode << 8) as i32) >> 6;
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::imm(op0 as u32)),
            reg_reserve!(),
            reg_reserve!(),
        )
    }

    #[inline]
    pub fn bl(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn swi(opcode: u32, op: Op) -> InstInfo {
        InstInfo::new(
            opcode,
            op,
            Operands::new_empty(),
            reg_reserve!(),
            reg_reserve!(),
        )
    }
}

pub use branch_ops::*;

mod branch_ops {
    use crate::jit::reg::{reg_reserve, Reg};
    use crate::jit::{InstInfo, Operand, Operands};

    #[inline]
    pub fn bx(opcode: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn blx_reg(opcode: u32) -> InstInfo {
        let op0 = Reg::from(opcode & 0xF);
        InstInfo {
            operands: Operands::new_1(Operand::reg(op0)),
            src_regs: reg_reserve!(op0),
            out_regs: reg_reserve!(),
        }
    }

    #[inline]
    pub fn b(opcode: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bl(opcode: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn swi(opcode: u32) -> InstInfo {
        todo!()
    }
}

pub use branch_ops::*;

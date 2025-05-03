mod branch_thumb_ops {
    use crate::jit::inst_info::{Operand, Operands};
    use crate::jit::inst_info_thumb::InstInfoThumb;
    use crate::jit::reg::{reg_reserve, Reg};
    use crate::jit::Op;

    #[inline]
    pub fn bx_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 3) & 0xF) as u8);
        InstInfoThumb::new(opcode, op, Operands::new_1(Operand::reg(op0)), reg_reserve!(op0), reg_reserve!(Reg::PC), 1)
    }

    #[inline]
    pub fn blx_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 3) & 0xF) as u8);
        InstInfoThumb::new(opcode, op, Operands::new_1(Operand::reg(op0)), reg_reserve!(op0), reg_reserve!(Reg::LR, Reg::PC), 1)
    }

    #[inline]
    pub fn b_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = (opcode << 5) as i16 >> 4; // * 2 (in steps of 2)
        InstInfoThumb::new(opcode, op, Operands::new_1(Operand::imm(op0 as u32)), reg_reserve!(), reg_reserve!(Reg::PC), 1)
    }

    #[inline]
    fn b_cond(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = (((opcode & 0xFF) as i8) as i32) << 1;
        InstInfoThumb::new(opcode, op, Operands::new_1(Operand::imm(op0 as u32)), reg_reserve!(Reg::CPSR), reg_reserve!(Reg::PC), 1)
    }

    #[inline]
    pub fn beq_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bne_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bcs_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bcc_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bmi_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bpl_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bvs_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bvc_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bhi_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bls_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bge_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn blt_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bgt_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn ble_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_cond(opcode, op)
    }

    #[inline]
    pub fn bl_setup_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = ((opcode as u32) << 21) as i32 >> 9;
        InstInfoThumb::new(opcode, op, Operands::new_1(Operand::imm(op0 as u32)), reg_reserve!(), reg_reserve!(), 1)
    }

    #[inline]
    pub fn bl_off_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = (opcode & 0x7FF) << 1; // * 2 (in steps of 2)
        InstInfoThumb::new(opcode, op, Operands::new_1(Operand::imm(op0 as u32)), reg_reserve!(), reg_reserve!(Reg::LR, Reg::PC), 1)
    }

    #[inline]
    pub fn blx_off_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = (opcode & 0x7FF) << 1; // * 2 (in steps of 2)
        InstInfoThumb::new(opcode, op, Operands::new_1(Operand::imm(op0 as u32)), reg_reserve!(), reg_reserve!(Reg::LR, Reg::PC), 1)
    }

    #[inline]
    pub fn swi_t(opcode: u16, op: Op) -> InstInfoThumb {
        InstInfoThumb::new(opcode, op, Operands::new_empty(), reg_reserve!(), reg_reserve!(), 3)
    }
}

pub(super) use branch_thumb_ops::*;

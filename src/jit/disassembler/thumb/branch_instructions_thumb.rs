mod branch_thumb_ops {
    use crate::jit::inst_info::{InstCycle, Operand, Operands};
    use crate::jit::inst_info_thumb::InstInfoThumb;
    use crate::jit::reg::{reg_reserve, Reg};
    use crate::jit::Op;

    #[inline]
    pub fn bx_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 3) & 0xF) as u8);
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            reg_reserve!(op0),
            reg_reserve!(),
            InstCycle::common(3),
        )
    }

    #[inline]
    pub fn blx_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn b_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = ((opcode & 0xFF) as i8) as i32 * 2;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_1(Operand::imm(op0 as u32)),
            reg_reserve!(),
            reg_reserve!(),
            InstCycle::common(3),
        )
    }

    #[inline]
    pub fn beq_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bne_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bcs_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bcc_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bmi_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bpl_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bvs_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bvc_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bhi_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bls_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bge_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn blt_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bgt_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn ble_t(opcode: u16, op: Op) -> InstInfoThumb {
        b_t(opcode, op)
    }

    #[inline]
    pub fn bl_setup_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = unsafe { (opcode as u32).unchecked_shl(21) } as i32 >> 9;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_1(Operand::imm(op0 as u32)),
            reg_reserve!(),
            reg_reserve!(Reg::LR),
            InstCycle::common(1),
        )
    }

    #[inline]
    pub fn bl_off_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = (opcode & 0x7FF) * 2; // * 2 (in steps of 2)
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_1(Operand::imm(op0 as u32)),
            reg_reserve!(),
            reg_reserve!(Reg::LR),
            InstCycle::common(3),
        )
    }

    #[inline]
    pub fn blx_off_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn swi_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }
}

pub use branch_thumb_ops::*;

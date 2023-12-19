mod transfer_thumb_ops {
    use crate::jit::inst_info::{InstCycle, Operand, Operands};
    use crate::jit::inst_info_thumb::InstInfoThumb;
    use crate::jit::reg::{reg_reserve, Reg, RegReserve};
    use crate::jit::Op;

    #[inline]
    pub fn ldrsb_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn ldrsh_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn ldrb_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn strb_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn ldrh_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn strh_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn ldr_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn str_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn ldrb_imm5_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn strb_imm5_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn ldrh_imm5_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from((opcode & 0x7) as u8);
        let op1 = Reg::from(((opcode >> 3) & 0x7) as u8);
        let op2 = (opcode >> 5) & 0x3E; // * 2 (in steps of 2)
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::imm(op2 as u32),
            ),
            reg_reserve!(op1),
            reg_reserve!(op0),
            InstCycle::new(1, 3),
        )
    }

    #[inline]
    pub fn strh_imm5_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from((opcode & 0x7) as u8);
        let op1 = Reg::from(((opcode >> 3) & 0x7) as u8);
        let op2 = (opcode >> 5) & 0x3E; // * 2 (in steps of 2)
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::imm(op2 as u32),
            ),
            reg_reserve!(op0, op1),
            reg_reserve!(),
            InstCycle::new(1, 2),
        )
    }

    #[inline]
    pub fn ldr_imm5_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from((opcode & 0x7) as u8);
        let op1 = Reg::from(((opcode >> 3) & 0x7) as u8);
        let op2 = (opcode >> 4) & 0x7C; // * 4 (in steps of 4)
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::imm(op2 as u32),
            ),
            reg_reserve!(op1),
            reg_reserve!(op0),
            InstCycle::new(1, 3),
        )
    }

    #[inline]
    pub fn str_imm5_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from((opcode & 0x7) as u8);
        let op1 = Reg::from(((opcode >> 3) & 0x7) as u8);
        let op2 = (opcode >> 4) & 0x7C; // * 4 (in steps of 4)
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::imm(op2 as u32),
            ),
            reg_reserve!(op0, op1),
            reg_reserve!(),
            InstCycle::new(1, 2),
        )
    }

    #[inline]
    pub fn ldr_pc_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 8) & 0x7) as u8);
        let op2 = (opcode & 0xFF) * 4;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(Reg::PC),
                Operand::imm(op2 as u32),
            ),
            reg_reserve!(Reg::PC),
            reg_reserve!(op0),
            InstCycle::new(1, 3),
        )
    }

    #[inline]
    pub fn ldr_sp_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn str_sp_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn ldmia_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 8) & 0x7) as u8);
        let rlist = RegReserve::from((opcode & 0xFF) as u32);
        let rlist_len = rlist.len() as u8;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            reg_reserve!(op0),
            rlist + op0,
            InstCycle::new(rlist_len + (rlist_len < 2) as u8, rlist_len + 2),
        )
    }

    #[inline]
    pub fn stmia_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn pop_t(opcode: u16, op: Op) -> InstInfoThumb {
        let rlist = RegReserve::from((opcode & 0xFF) as u32);
        let rlist_len = rlist.len() as u8;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(Reg::SP)),
            reg_reserve!(Reg::SP),
            rlist + Reg::SP,
            InstCycle::new(rlist_len + (rlist_len < 2) as u8, rlist_len + 2),
        )
    }

    #[inline]
    pub fn push_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn pop_pc_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }

    #[inline]
    pub fn push_lr_t(opcode: u16, op: Op) -> InstInfoThumb {
        let rlist = RegReserve::from((opcode & 0xFF) as u32) + Reg::LR;
        let rlist_len = rlist.len() as u8;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(Reg::SP)),
            rlist + Reg::SP,
            reg_reserve!(Reg::SP),
            InstCycle::new(rlist_len + (rlist_len < 2) as u8, rlist_len + 2),
        )
    }
}

pub use transfer_thumb_ops::*;

mod transfer_thumb_ops {
    use crate::jit::inst_info::{Operand, Operands};
    use crate::jit::inst_info_thumb::InstInfoThumb;
    use crate::jit::reg::{reg_reserve, Reg, RegReserve};
    use crate::jit::Op;

    #[inline]
    pub fn ldrsb_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from((opcode & 0x7) as u8);
        let op1 = Reg::from(((opcode >> 3) & 0x7) as u8);
        let op2 = Reg::from(((opcode >> 6) & 0x7) as u8);
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2)),
            reg_reserve!(op1, op2),
            reg_reserve!(op0),
            3,
        )
    }

    #[inline]
    pub fn ldrsh_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        ldrsb_reg_t(opcode, op)
    }

    #[inline]
    pub fn ldrb_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        ldrsb_reg_t(opcode, op)
    }

    #[inline]
    pub fn strb_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from((opcode & 0x7) as u8);
        let op1 = Reg::from(((opcode >> 3) & 0x7) as u8);
        let op2 = Reg::from(((opcode >> 6) & 0x7) as u8);
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::reg(op2)),
            reg_reserve!(op0, op1, op2),
            reg_reserve!(),
            2,
        )
    }

    #[inline]
    pub fn ldrh_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        ldrsb_reg_t(opcode, op)
    }

    #[inline]
    pub fn strh_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        strb_reg_t(opcode, op)
    }

    #[inline]
    pub fn ldr_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        ldrsb_reg_t(opcode, op)
    }

    #[inline]
    pub fn str_reg_t(opcode: u16, op: Op) -> InstInfoThumb {
        strb_reg_t(opcode, op)
    }

    #[inline]
    pub fn ldrb_imm5_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from((opcode & 0x7) as u8);
        let op1 = Reg::from(((opcode >> 3) & 0x7) as u8);
        let op2 = (opcode & 0x07C0) >> 6;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(op2 as u32)),
            reg_reserve!(op1),
            reg_reserve!(op0),
            3,
        )
    }

    #[inline]
    pub fn strb_imm5_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from((opcode & 0x7) as u8);
        let op1 = Reg::from(((opcode >> 3) & 0x7) as u8);
        let op2 = (opcode & 0x07C0) >> 6;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(op2 as u32)),
            reg_reserve!(op0, op1),
            reg_reserve!(),
            2,
        )
    }

    #[inline]
    pub fn ldrh_imm5_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from((opcode & 0x7) as u8);
        let op1 = Reg::from(((opcode >> 3) & 0x7) as u8);
        let op2 = (opcode >> 5) & 0x3E; // * 2 (in steps of 2)
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(op2 as u32)),
            reg_reserve!(op1),
            reg_reserve!(op0),
            3,
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
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(op2 as u32)),
            reg_reserve!(op0, op1),
            reg_reserve!(),
            2,
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
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(op2 as u32)),
            reg_reserve!(op1),
            reg_reserve!(op0),
            3,
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
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(op2 as u32)),
            reg_reserve!(op0, op1),
            reg_reserve!(),
            2,
        )
    }

    #[inline]
    pub fn ldr_pc_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 8) & 0x7) as u8);
        let op2 = (opcode & 0xFF) << 2;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(Reg::PC), Operand::imm(op2 as u32)),
            reg_reserve!(Reg::PC),
            reg_reserve!(op0),
            3,
        )
    }

    #[inline]
    pub fn ldr_sp_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 8) & 0x7) as u8);
        let op2 = (opcode & 0xFF) << 2;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(Reg::SP), Operand::imm(op2 as u32)),
            reg_reserve!(Reg::SP),
            reg_reserve!(op0),
            3,
        )
    }

    #[inline]
    pub fn str_sp_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 8) & 0x7) as u8);
        let op2 = (opcode & 0xFF) << 2;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(Reg::SP), Operand::imm(op2 as u32)),
            reg_reserve!(op0, Reg::SP),
            reg_reserve!(),
            2,
        )
    }

    #[inline]
    pub fn ldmia_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 8) & 0x7) as u8);
        let rlist = RegReserve::from((opcode & 0xFF) as u32);
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            reg_reserve!(op0),
            rlist + op0,
            rlist.len() as u8 + 2,
        )
    }

    #[inline]
    pub fn stmia_t(opcode: u16, op: Op) -> InstInfoThumb {
        let op0 = Reg::from(((opcode >> 8) & 0x7) as u8);
        let rlist = RegReserve::from((opcode & 0xFF) as u32);
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::reg_list(rlist)),
            reg_reserve!(op0) + rlist,
            reg_reserve!(op0),
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn pop_t(opcode: u16, op: Op) -> InstInfoThumb {
        let rlist = RegReserve::from((opcode & 0xFF) as u32);
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(Reg::SP), Operand::reg_list(rlist)),
            reg_reserve!(Reg::SP),
            rlist + Reg::SP,
            rlist.len() as u8 + 2,
        )
    }

    #[inline]
    pub fn push_t(opcode: u16, op: Op) -> InstInfoThumb {
        let rlist = RegReserve::from((opcode & 0xFF) as u32);
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(Reg::SP), Operand::reg_list(rlist)),
            rlist + Reg::SP,
            reg_reserve!(Reg::SP),
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn pop_pc_t(opcode: u16, op: Op) -> InstInfoThumb {
        let rlist = RegReserve::from((opcode & 0xFF) as u32) + Reg::PC;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(Reg::SP), Operand::reg_list(rlist)),
            reg_reserve!(Reg::SP),
            rlist + Reg::SP,
            rlist.len() as u8 + 2,
        )
    }

    #[inline]
    pub fn push_lr_t(opcode: u16, op: Op) -> InstInfoThumb {
        let rlist = RegReserve::from((opcode & 0xFF) as u32) + Reg::LR;
        InstInfoThumb::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(Reg::SP), Operand::reg_list(rlist)),
            rlist + Reg::SP,
            reg_reserve!(Reg::SP),
            rlist.len() as u8 + 1,
        )
    }
}

pub(super) use transfer_thumb_ops::*;

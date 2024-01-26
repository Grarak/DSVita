mod transfer_variations {
    use crate::jit::reg::Reg;

    #[inline]
    pub fn ip(opcode: u32) -> u32 {
        opcode & 0xFFF
    }

    #[inline]
    pub fn ip_h(opcode: u32) -> u32 {
        ((opcode >> 4) & 0xF0) | (opcode & 0xF)
    }

    #[inline]
    pub fn rp(opcode: u32) -> Reg {
        Reg::from((opcode & 0xF) as u8)
    }

    #[inline]
    pub fn imm_shift(opcode: u32) -> (Reg, u8) {
        let reg = Reg::from((opcode & 0xF) as u8);
        let shift = ((opcode >> 7) & 0x1F) as u8;
        return (reg, shift);
    }
}

pub(super) use transfer_variations::*;

mod transfer_ops {
    use crate::jit::inst_info::{InstInfo, Operand, Operands};
    use crate::jit::reg::{reg_reserve, Reg, RegReserve};
    use crate::jit::{Op, ShiftType};

    // Half

    #[inline]
    pub fn ldrsb_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_ofrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_ofrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_prrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_prrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_ptrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_ptrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_ofrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_ofrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_prrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_prrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_ptrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_ptrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(op0),
            3,
        )
    }

    #[inline]
    pub fn ldrh_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_ofrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_ofrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_prrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_prrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_ptrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_ptrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op0, op1),
            reg_reserve!(),
            2,
        )
    }

    #[inline]
    pub fn strh_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_ofrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_ofrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_prrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_prrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_ptrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_ptrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_ofrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_ofrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_prrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_prrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_ptrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_ptrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_ofrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_ofrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_prrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_prrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_ptrm_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_ptrp_impl(opcode: u32, op: Op, operand2: Reg) -> InstInfo {
        todo!()
    }

    // Full

    #[inline]
    pub fn ldrb_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(op0, op1),
            3,
        )
    }

    #[inline]
    pub fn ldrb_ofrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ofrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ofrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ofrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ofrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::Lsl, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(op0),
            3,
        )
    }

    #[inline]
    pub fn ldrb_ofrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::Lsr, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(op0),
            3,
        )
    }

    #[inline]
    pub fn ldrb_ofrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ofrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_prrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_prrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_prrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_prrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_prrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_prrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_prrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_prrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ptrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ptrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ptrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ptrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ptrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ptrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ptrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_ptrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op0, op1),
            reg_reserve!(),
            2,
        )
    }

    #[inline]
    pub fn strb_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op0, op1),
            reg_reserve!(op1),
            2,
        )
    }

    #[inline]
    pub fn strb_ofrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ofrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ofrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ofrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ofrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ofrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ofrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ofrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_prrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_prrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_prrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_prrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_prrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_prrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_prrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_prrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ptrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ptrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ptrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ptrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ptrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ptrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ptrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_ptrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(op0),
            3,
        )
    }

    #[inline]
    pub fn ldr_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(op0, op1),
            3,
        )
    }

    #[inline]
    pub fn ldr_ofrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ofrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ofrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ofrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ofrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::Lsl, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(op0),
            3,
        )
    }

    #[inline]
    pub fn ldr_ofrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ofrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ofrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_prrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_prrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_prrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_prrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_prrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_prrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_prrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_prrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ptrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ptrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ptrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ptrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ptrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ptrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ptrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_ptrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op0, op1),
            reg_reserve!(),
            2,
        )
    }

    #[inline]
    pub fn str_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op0, op1),
            reg_reserve!(op1),
            2,
        )
    }

    #[inline]
    pub fn str_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ofrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ofrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ofrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ofrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ofrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ofrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ofrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ofrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_prrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_prrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_prrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_prrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_prrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_prrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_prrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_prrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ptrmll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ptrmlr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ptrmar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ptrmrr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ptrpll_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ptrplr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ptrpar_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_ptrprr_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn swpb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn swp(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmda(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmda(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmia(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            reg_reserve!(op0),
            rlist,
            rlist.len() as u8 + 2,
        )
    }

    #[inline]
    pub fn stmia(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            rlist + op0,
            reg_reserve!(),
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn ldmdb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmdb(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            rlist + op0,
            reg_reserve!(),
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn ldmib(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmib(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmda_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmda_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmia_w(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            reg_reserve!(op0),
            rlist + op0,
            rlist.len() as u8 + 2,
        )
    }

    #[inline]
    pub fn stmia_w(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            rlist + op0,
            reg_reserve!(op0),
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn ldmdb_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmdb_w(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 16) & 0xF) as u8);
        let rlist = RegReserve::from(opcode & 0xFFFF);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            rlist + op0,
            reg_reserve!(op0),
            rlist.len() as u8 + 1,
        )
    }

    #[inline]
    pub fn ldmib_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmib_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmda_u(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmda_u(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmia_u(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmia_u(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmdb_u(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmdb_u(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmib_u(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmib_u(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmda_u_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmda_u_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmia_u_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmia_u_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmdb_u_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmdb_u_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmib_u_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmib_u_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn msr_rc(opcode: u32, op: Op) -> InstInfo {
        let op1 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op1)),
            reg_reserve!(op1),
            reg_reserve!(Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn msr_rs(opcode: u32, op: Op) -> InstInfo {
        let op1 = Reg::from((opcode & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op1)),
            reg_reserve!(op1),
            reg_reserve!(Reg::SPSR),
            1,
        )
    }

    #[inline]
    pub fn msr_ic(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn msr_is(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mrs_rc(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            reg_reserve!(Reg::CPSR),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn mrs_rs(opcode: u32, op: Op) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op0)),
            reg_reserve!(Reg::SPSR),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn mrc(opcode: u32, op: Op) -> InstInfo {
        let op2 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op2)),
            reg_reserve!(),
            reg_reserve!(op2),
            1,
        )
    }

    #[inline]
    pub fn mcr(opcode: u32, op: Op) -> InstInfo {
        let op2 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_1(Operand::reg(op2)),
            reg_reserve!(op2),
            reg_reserve!(),
            1,
        )
    }
}

pub(super) use transfer_ops::*;

mod alu_variations {
    use crate::jit::reg::Reg;

    #[inline]
    pub fn imm_shift(opcode: u32) -> (Reg, u8) {
        let reg = Reg::from((opcode & 0xF) as u8);
        let shift = (opcode >> 7) & 0x1F;
        (reg, shift as u8)
    }

    #[inline]
    pub fn reg_shift(opcode: u32) -> (Reg, Reg) {
        let reg = Reg::from((opcode & 0xF) as u8);
        let shift = Reg::from(((opcode >> 8) & 0xF) as u8);
        (reg, shift)
    }

    #[inline]
    pub fn imm(opcode: u32) -> u32 {
        let value = opcode & 0xFF;
        let shift = (opcode >> 7) & 0x1E;
        unsafe { value.unchecked_shl(32 - shift) | (value >> shift) }
    }
}

pub use alu_variations::*;

mod alu_ops {
    use crate::jit::inst_info::{InstInfo, Operand, Operands};
    use crate::jit::reg::{reg_reserve, Reg};
    use crate::jit::{Op, ShiftType};

    #[inline]
    pub fn _and_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::LSL, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn _and_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn _and_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn _and_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn _and_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn _and_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn _and_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn _and_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn _and_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn ands_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ands_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ands_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ands_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ands_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ands_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ands_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ands_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ands_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eor_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eor_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eor_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eor_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eor_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eor_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eor_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eor_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eor_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eors_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eors_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eors_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eors_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eors_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eors_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eors_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eors_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn eors_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sub_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::LSL, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn sub_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sub_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sub_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sub_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sub_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sub_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sub_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sub_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn subs_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::LSL, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn subs_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn subs_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn subs_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn subs_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn subs_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn subs_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn subs_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn subs_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(op0, Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn rsb_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsb_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsb_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsb_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsb_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsb_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsb_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsb_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsb_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsbs_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsbs_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsbs_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsbs_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsbs_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsbs_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsbs_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsbs_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsbs_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn add_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::LSL, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn add_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn add_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn add_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn add_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn add_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn add_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn add_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn add_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn adds_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adds_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adds_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adds_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adds_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adds_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adds_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adds_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adds_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adc_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adc_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adc_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adc_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adc_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adc_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adc_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adc_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adc_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adcs_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adcs_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adcs_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adcs_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adcs_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adcs_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adcs_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adcs_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn adcs_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbc_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbc_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbc_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbc_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbc_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbc_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbc_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbc_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbc_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbcs_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbcs_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbcs_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbcs_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbcs_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbcs_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbcs_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbcs_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn sbcs_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsc_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsc_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsc_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsc_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsc_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsc_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsc_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsc_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rsc_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rscs_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rscs_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rscs_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rscs_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rscs_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rscs_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rscs_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rscs_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn rscs_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn tst_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn tst_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn tst_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn tst_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn tst_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn tst_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn tst_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn tst_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn tst_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn teq_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn teq_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn teq_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn teq_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn teq_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn teq_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn teq_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn teq_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn teq_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmp_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::LSL, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn cmp_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmp_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmp_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmp_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmp_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmp_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmp_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmp_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn cmn_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmn_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmn_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmn_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmn_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmn_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmn_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmn_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn cmn_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orr_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::LSL, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn orr_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orr_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orr_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orr_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orr_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orr_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orr_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orr_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(Operand::reg(op0), Operand::reg(op1), Operand::imm(operand2)),
            reg_reserve!(op1),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn orrs_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orrs_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orrs_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orrs_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orrs_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orrs_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orrs_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orrs_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn orrs_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mov_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(
                Operand::reg(op0),
                Operand::reg_imm_shift(operand2.0, ShiftType::LSL, operand2.1),
            ),
            reg_reserve!(operand2.0),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn mov_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mov_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(
                Operand::reg(op0),
                Operand::reg_imm_shift(operand2.0, ShiftType::LSR, operand2.1),
            ),
            reg_reserve!(operand2.0),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn mov_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mov_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mov_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mov_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mov_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mov_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_2(Operand::reg(op0), Operand::imm(operand2)),
            reg_reserve!(),
            reg_reserve!(op0),
            1,
        )
    }

    #[inline]
    pub fn movs_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn movs_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn movs_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn movs_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn movs_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn movs_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn movs_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn movs_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn movs_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bic_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bic_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bic_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bic_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bic_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bic_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bic_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bic_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bic_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bics_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        let op0 = Reg::from(((opcode >> 12) & 0xF) as u8);
        let op1 = Reg::from(((opcode >> 16) & 0xF) as u8);
        InstInfo::new(
            opcode,
            op,
            Operands::new_3(
                Operand::reg(op0),
                Operand::reg(op1),
                Operand::reg_imm_shift(operand2.0, ShiftType::LSL, operand2.1),
            ),
            reg_reserve!(op1, operand2.0),
            reg_reserve!(op0, Reg::CPSR),
            1,
        )
    }

    #[inline]
    pub fn bics_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bics_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bics_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bics_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bics_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bics_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bics_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn bics_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvn_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvn_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvn_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvn_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvn_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvn_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvn_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvn_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvn_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvns_lli_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvns_llr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvns_lri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvns_lrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvns_ari_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvns_arr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvns_rri_impl(opcode: u32, op: Op, operand2: (Reg, u8)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvns_rrr_impl(opcode: u32, op: Op, operand2: (Reg, Reg)) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mvns_imm_impl(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mul(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mla(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn umull(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn umlal(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smull(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlal(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn muls(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mlas(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn umulls(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn umlals(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smulls(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlals(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smulbb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smulbt(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smultb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smultt(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smulwb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smulwt(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlabb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlabt(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlatb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlatt(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlawb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlawt(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlalbb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlalbt(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlaltb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn smlaltt(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn qadd(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn qsub(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn qdadd(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn qdsub(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn clz(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }
}

pub use alu_ops::*;

mod alu_variations {
    #[inline]
    pub fn lli(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn llr(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn lri(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn lrr(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn ari(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn arr(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn rri(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn rrr(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn imm(opcode: u32) -> u32 {
        // Rotate 8-bit immediate right by a multiple of 2
        let value = opcode & 0xFF;
        let shift = (opcode >> 7) & 0x1E;
        (value << (32 - shift)) | (value >> shift)
    }

    #[inline]
    pub fn lli_s(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn llr_s(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn lri_s(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn lrr_s(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn ari_s(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn arr_s(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn rri_s(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn rrr_s(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn imm_s(opcode: u32) -> u32 {
        todo!()
    }
}

pub use alu_variations::*;

mod alu_ops {
    use crate::jit::disassembler::{InstInfo, Op, Operand, Reg};
    use crate::jit::jit::JitAsm;
    use std::mem;

    pub fn _and(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn eor(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn sub(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn rsb(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn add(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn adc(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn sbc(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn rsc(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn tst(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn teq(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn cmp(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn cmn(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn orr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn mov(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        let operand0 = opcode >> 12;
        InstInfo {
            name,
            op: Op::MOV,
            operands: [
                Operand::Reg(Reg::from(operand0)),
                Operand::Imm(operand2),
                Operand::None,
                Operand::None,
                Operand::None,
            ],
            operands_count: 2,
        }
    }

    pub fn bic(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn mvn(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ands(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn eors(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn subs(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn rsbs(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn adds(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn adcs(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn sbcs(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn rscs(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn orrs(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn movs(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn bics(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn mvns(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn mul(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mla(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn umull(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn umlal(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smull(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlal(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn muls(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mlas(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn umulls(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn umlals(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smulls(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlals(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smulbb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smulbt(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smultb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smultt(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smulwb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smulwt(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlabb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlabt(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlatb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlatt(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlawb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlawt(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlalbb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlalbt(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlaltb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn smlaltt(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn qadd(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn qsub(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn qdadd(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn qdsub(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn clz(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn add_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn sub_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn add_h_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn cmp_h_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mov_h_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn add_pc_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn add_sp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn add_sp_imm_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn lsl_imm_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn lsr_imm_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn asr_imm_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn add_imm3_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn sub_imm3_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn add_imm8_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn sub_imm8_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn cmp_imm8_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mov_imm8_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn lsl_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn lsr_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn asr_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ror_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn and_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn eor_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn adc_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn sbc_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn tst_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn cmp_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn cmn_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn orr_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bic_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mvn_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn neg_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mul_dp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }
}

pub use alu_ops::*;

mod transfer_variations {
    #[inline]
    pub fn ip(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn ip_h(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn rp(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn rpll(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn rplr(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn rpar(opcode: u32) -> u32 {
        todo!()
    }

    #[inline]
    pub fn rprr(opcode: u32) -> u32 {
        todo!()
    }
}

pub use transfer_variations::*;

mod transfer_ops {
    use crate::jit::disassembler::InstInfo;
    use crate::jit::jit::JitAsm;

    pub fn ldrsb_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrsh_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrb_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn strb_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrh_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn strh_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldr_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn str_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrd_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn strd_of(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrsb_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrsh_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrb_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn strb_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrh_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn strh_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldr_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn str_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrd_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn strd_pr(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrsb_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrsh_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrb_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn strb_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrh_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn strh_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldr_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn str_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrd_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn strd_pt(asm: &mut JitAsm, name: &'static str, opcode: u32, operand2: u32) -> InstInfo {
        todo!()
    }

    pub fn swpb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn swp(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmda(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmda(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmia(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmia(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmdb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmdb(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmib(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmib(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmda_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmda_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmia_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmia_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmdb_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmdb_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmib_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmib_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmda_u(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmda_u(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmia_u(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmia_u(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmdb_u(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmdb_u(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmib_u(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmib_u(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmda_u_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmda_u_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmia_u_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmia_u_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmdb_u_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmdb_u_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmib_u_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmib_u_w(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn msr_rc(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn msr_rs(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn msr_ic(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn msr_is(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mrs_rc(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mrs_rs(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mrc(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn mcr(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrsb_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrsh_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrb_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn strb_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrh_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn strh_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldr_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn str_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrb_imm5_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn strb_imm5_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldrh_imm5_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn strh_imm5_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldr_imm5_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn str_imm5_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldr_pc_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldr_sp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn str_sp_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ldmia_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn stmia_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn pop_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn push_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn pop_pc_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn push_lr_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }
}

pub use transfer_ops::*;

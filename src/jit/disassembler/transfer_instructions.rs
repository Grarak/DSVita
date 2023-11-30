mod transfer_variations {

    #[inline]
    pub fn ip(opcode: u32) -> u32 {
        opcode & 0xFFF
    }

    #[inline]
    pub fn ip_h(opcode: u32) -> u32 {
        ((opcode >> 4) & 0xF0) | (opcode & 0xF)
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
    use crate::jit::inst_info::{InstInfo, Operand, Operands};
    use crate::jit::reg::{reg_reserve, Reg};
    use crate::jit::Op;

    #[inline]
    pub fn ldrsb_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
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
        )
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
        )
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
        )
    }

    #[inline]
    pub fn ldrd_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_of(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_pr(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsb_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrsh_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrb_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strb_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrh_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strh_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldr_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn str_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldrd_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn strd_pt(opcode: u32, op: Op, operand2: u32) -> InstInfo {
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
        todo!()
    }

    #[inline]
    pub fn stmia(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmdb(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmdb(opcode: u32, op: Op) -> InstInfo {
        todo!()
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
        todo!()
    }

    #[inline]
    pub fn stmia_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn ldmdb_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn stmdb_w(opcode: u32, op: Op) -> InstInfo {
        todo!()
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
            Operands::new_2(Operand::reg(Reg::CPSR), Operand::reg(op1)),
            reg_reserve!(op1),
            reg_reserve!(Reg::CPSR),
        )
    }

    #[inline]
    pub fn msr_rs(opcode: u32, op: Op) -> InstInfo {
        todo!()
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
        todo!()
    }

    #[inline]
    pub fn mrs_rs(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }

    #[inline]
    pub fn mrc(opcode: u32, op: Op) -> InstInfo {
        todo!()
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
        )
    }
}

pub use transfer_ops::*;

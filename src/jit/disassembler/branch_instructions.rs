mod branch_ops {
    use crate::jit::disassembler::InstInfo;
    use crate::jit::jit::JitAsm;

    pub fn bx(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn blx_reg(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn b(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bl(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn blx(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn swi(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bx_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn blx_reg_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn b_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn beq_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bne_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bcs_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bcc_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bmi_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bpl_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bvs_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bvc_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bhi_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bls_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bge_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn blt_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bgt_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn ble_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bl_setup_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn bl_off_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn blx_off_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }

    pub fn swi_t(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }
}

pub use branch_ops::*;

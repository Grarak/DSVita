use std::mem;

mod alu_instructions;
mod branch_instructions;
mod delegations;
pub mod lookup_table;
mod transfer_instructions;

#[derive(Debug)]
pub struct InstInfo {
    pub name: &'static str,
    pub op: Op,
    pub operands: [Operand; 5],
    operands_count: u8,
}

#[derive(Debug)]
pub enum Op {
    MOV,
}

#[derive(Debug)]
pub enum Reg {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    R9,
    R10,
    R11,
    R12,
    SP,
    LR,
    PC,
    CPSR = 16,
}

impl From<u32> for Reg {
    fn from(value: u32) -> Self {
        Reg::from(value as u8)
    }
}

impl From<u8> for Reg {
    fn from(value: u8) -> Self {
        if value >= Reg::CPSR as u8 {
            panic!("Can't map {} to register", value)
        }
        unsafe { mem::transmute(value) }
    }
}

#[derive(Debug)]
pub enum Operand {
    Reg(Reg),
    Imm(u32),
    None,
}

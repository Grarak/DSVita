use crate::jit::inst_info::Operands;
use crate::jit::reg::RegReserve;
use crate::jit::Op;

#[derive(Clone)]
pub struct InstInfoThumb {
    pub opcode: u16,
    pub op: Op,
    pub operands: Operands,
    pub src_regs: RegReserve,
    pub out_regs: RegReserve,
    pub cycle: u8,
}

impl InstInfoThumb {
    pub fn new(opcode: u16, op: Op, operands: Operands, src_regs: RegReserve, out_regs: RegReserve, cycle: u8) -> Self {
        InstInfoThumb {
            opcode,
            op,
            operands,
            src_regs,
            out_regs,
            cycle,
        }
    }
}

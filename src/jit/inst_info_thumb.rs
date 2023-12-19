use crate::jit::inst_info::{InstCycle, Operands};
use crate::jit::reg::RegReserve;
use crate::jit::Op;

#[derive(Copy, Clone, Debug)]
pub struct InstInfoThumb {
    pub opcode: u16,
    pub op: Op,
    pub operands: Operands,
    pub src_regs: RegReserve,
    pub out_regs: RegReserve,
    pub cycle: InstCycle,
}

impl InstInfoThumb {
    pub fn new(
        opcode: u16,
        op: Op,
        operands: Operands,
        src_regs: RegReserve,
        out_regs: RegReserve,
        cycle: InstCycle,
    ) -> Self {
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

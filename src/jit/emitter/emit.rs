use crate::jit::jit::JitAsm;
use crate::jit::reg::{GpRegReserve, Reg, RegReserve};
use crate::jit::{InstInfo, Op};
use crate::logging::debug_println;

impl JitAsm {
    pub fn emit(&mut self, buf_index: usize, pc: u32) -> bool {
        let (opcode, op, _) = &self.opcode_buf[buf_index];
        debug_println!("{:?} as {:x}", op, pc);

        let emit_func = match op {
            Op::StrOfip => JitAsm::emit_str,
            Op::BlxReg => JitAsm::emit_blx,
            _ => {
                self.jit_memory.write(*opcode);
                |_: &mut JitAsm, _: usize, _: u32| true
            }
        };

        emit_func(self, buf_index, pc)
    }
}

pub fn get_writable_gp_regs(
    num: u8,
    mut reserve: RegReserve,
    insts: &[(u32, Op, InstInfo)],
) -> Result<Vec<Reg>, ()> {
    let mut writable_regs = Vec::new();
    writable_regs.reserve(num as usize);
    for (_, _, inst) in insts {
        reserve += inst.src_regs;
        let writable = reserve ^ inst.out_regs;
        for reg in GpRegReserve::from(writable) {
            writable_regs.push(reg);
            if writable_regs.len() >= num as usize {
                return Ok(writable_regs);
            }
        }
    }
    Err(())
}

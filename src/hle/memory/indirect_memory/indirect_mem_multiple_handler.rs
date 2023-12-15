use crate::hle::memory::indirect_memory::indirect_mem_handler::IndirectMemHandler;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::Op;
use crate::logging::debug_println;

impl IndirectMemHandler {
    fn handle_multiple_request(&mut self, opcode: u32, pc: u32, thumb: bool, write: bool) {
        debug_println!(
            "handle multiple request at {:x} thumb: {} write: {}",
            pc,
            thumb,
            write
        );

        let inst_info = {
            if thumb {
                let (op, func) = lookup_thumb_opcode(opcode as u16);
                InstInfo::from(&func(opcode as u16, *op))
            } else {
                let (op, func) = lookup_opcode(opcode);
                func(opcode, *op)
            }
        };

        let mut pre = match inst_info.op {
            Op::Ldmia | Op::LdmiaW | Op::StmiaW | Op::LdmiaT | Op::PopT => false,
            Op::PushLrT => true,
            _ => todo!("{:?}", inst_info),
        };

        let decrement = match inst_info.op {
            Op::Ldmia | Op::LdmiaW | Op::StmiaW | Op::LdmiaT | Op::PopT => false,
            Op::PushLrT => {
                pre = !pre;
                true
            }
            _ => todo!("{:?}", inst_info),
        };

        let write_back = match inst_info.op {
            Op::Ldmia => false,
            Op::LdmiaW | Op::StmiaW | Op::PushLrT | Op::LdmiaT | Op::PopT => true,
            _ => todo!("{:?}", inst_info),
        };

        let operands = inst_info.operands();

        let op0 = operands[0].as_reg_no_shift().unwrap();
        let mut rlist = RegReserve::from(inst_info.opcode & if thumb { 0xFF } else { 0xFFFF });
        if inst_info.op == Op::PushLrT {
            rlist += Reg::LR;
        }

        if rlist.len() == 0 {
            todo!()
        }

        if rlist.is_reserved(*op0) {
            todo!()
        }

        if rlist.is_reserved(Reg::PC) {
            todo!()
        }

        if *op0 == Reg::PC {
            todo!()
        }

        let start_addr = *self.thread_regs.borrow().get_reg_value(*op0);
        let mut addr = start_addr - (decrement as u32 * rlist.len() as u32 * 4);

        // TODO use batches
        for reg in rlist {
            addr += pre as u32 * 4;
            if write {
                let value = *self.thread_regs.borrow_mut().get_reg_value(reg);
                self.write(addr, value);
            } else {
                let value = self.read(addr);
                *self.thread_regs.borrow_mut().get_reg_value_mut(reg) = value;
            }
            addr += !pre as u32 * 4;
        }

        if write_back {
            *self.thread_regs.borrow_mut().get_reg_value_mut(*op0) = (decrement as u32 * (start_addr - rlist.len() as u32 * 4)) // decrement
                + (!decrement as u32 * addr); // increment
        }
    }
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_read_multiple(
    handler: *mut IndirectMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request(opcode, pc, false, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_write_multiple(
    handler: *mut IndirectMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request(opcode, pc, false, true);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_read_multiple_thumb(
    handler: *mut IndirectMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request(opcode, pc, true, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_write_multiple_thumb(
    handler: *mut IndirectMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request(opcode, pc, true, true);
}

use crate::hle::memory::indirect_memory::indirect_mem_handler::IndirectMemHandler;
use crate::jit::reg::RegReserve;
use crate::jit::Op;
use crate::logging::debug_println;

impl IndirectMemHandler {
    fn handle_multiple_request(&self, pc: u32, write: bool) {
        debug_println!(
            "indirect memory multiple {} {:x}",
            if write { "write" } else { "read" },
            pc
        );

        let inst_info = {
            let vmm = self.vmm.borrow();
            IndirectMemHandler::get_inst_info(&vmm.get_vm_mapping(), pc)
        };

        let pre = match inst_info.op {
            Op::Ldmia | Op::LdmiaW | Op::StmiaW => false,
            _ => todo!(),
        };

        let decrement = match inst_info.op {
            Op::Ldmia | Op::LdmiaW => false,
            Op::StmiaW => true,
            _ => todo!(),
        };

        let write_back = match inst_info.op {
            Op::Ldmia => false,
            Op::LdmiaW | Op::StmiaW => true,
            _ => todo!(),
        };

        let operands = inst_info.operands();

        let op0 = operands[0].as_reg_no_shift().unwrap();
        let rlist = RegReserve::from(inst_info.opcode & 0xFFFF);

        if rlist.len() == 0 {
            todo!()
        }

        if rlist.is_reserved(*op0) {
            todo!()
        }

        let start_addr = *self.thread_regs.borrow().get_reg_value(*op0);
        let mut addr = start_addr;

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
            *self.thread_regs.borrow_mut().get_reg_value_mut(*op0) = (decrement as u32 * (rlist.len() as u32 * 4 + start_addr)) // stm 
                    + (!decrement as u32 * addr); // ldm
        }
    }
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_read_multiple(handler: *const IndirectMemHandler, pc: u32) {
    (*handler).handle_multiple_request(pc, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_write_multiple(handler: *const IndirectMemHandler, pc: u32) {
    (*handler).handle_multiple_request(pc, true);
}

use crate::hle::indirect_memory::MemoryAmount;
use crate::hle::registers::ThreadRegs;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::inst_info::{InstInfo, Operand, ShiftValue};
use crate::jit::reg::Reg;
use crate::jit::{Op, ShiftType};
use crate::logging::debug_println;
use crate::memory::{VMmap, VmManager};
use std::cell::RefCell;
use std::rc::Rc;

pub struct IndirectMemoryHandler {
    pub vmm: Rc<RefCell<VmManager>>,
    pub thread_regs: Rc<RefCell<ThreadRegs>>,
}

impl IndirectMemoryHandler {
    pub fn new(vmm: Rc<RefCell<VmManager>>, thread_regs: Rc<RefCell<ThreadRegs>>) -> Self {
        IndirectMemoryHandler { vmm, thread_regs }
    }

    pub fn get_inst_info(vmmap: &VMmap, addr: u32) -> InstInfo {
        let (_, aligned, _) = unsafe { vmmap[addr as usize..].align_to::<u32>() };
        let opcode = aligned[0];
        let (op, func) = lookup_opcode(opcode);
        func(opcode, *op)
    }

    fn handle_request(&self, pc: u32, write: bool) {
        debug_println!(
            "indirect memory {} {:x}",
            if write { "write" } else { "read" },
            pc
        );

        let vmm = self.vmm.borrow();
        let mut vmmap = vmm.get_vm_mapping();

        let inst_info = IndirectMemoryHandler::get_inst_info(&vmmap, pc);

        let pre = match inst_info.op {
            Op::LdrOfip | Op::StrOfip => true,
            _ => todo!(),
        };

        let add_to_base = match inst_info.op {
            Op::LdrOfip | Op::StrOfip => true,
            _ => todo!(),
        };

        let write_back = match inst_info.op {
            Op::LdrOfip | Op::StrOfip => false,
            _ => todo!(),
        };

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op1 = operands[1].as_reg_no_shift().unwrap();

        let get_reg_value = |reg: Reg| match reg {
            Reg::PC => pc + 8,
            _ => *self.thread_regs.borrow().get_reg_value(reg),
        };
        let set_reg_value = |reg: Reg, value: u32| match reg {
            Reg::PC => todo!(),
            _ => *self.thread_regs.borrow_mut().get_reg_value_mut(reg) = value,
        };

        let new_base =
            {
                let mut op1_value = get_reg_value(*op1);

                if operands.len() == 3 {
                    let op2_value = match &operands[2] {
                        Operand::Reg { reg, shift } => {
                            let mut op2_value = get_reg_value(*reg);

                            if let Some(shift) = shift {
                                let (shift_type, shift) = (*shift).into();
                                let shift_value = match shift {
                                    ShiftValue::Reg(reg) => get_reg_value(reg),
                                    ShiftValue::Imm(imm) => imm as u32,
                                };

                                match shift_type {
                                    ShiftType::LSL => {
                                        op2_value = unsafe { op2_value.unchecked_shl(shift_value) }
                                    }
                                    ShiftType::LSR => {
                                        op2_value = unsafe { op2_value.unchecked_shr(shift_value) }
                                    }
                                    ShiftType::ASR => {
                                        op2_value =
                                            unsafe { (op2_value as i32).unchecked_shr(shift_value) }
                                                as u32
                                    }
                                    ShiftType::ROR => {
                                        op2_value = unsafe {
                                            op2_value.unchecked_shr(shift_value)
                                                | op2_value.unchecked_shl(32 - shift_value)
                                        };
                                    }
                                }
                            }

                            op2_value
                        }
                        Operand::Imm(imm) => *imm,
                        _ => panic!(),
                    };

                    if add_to_base {
                        op1_value += op2_value;
                    } else {
                        op1_value -= op2_value;
                    }
                }

                op1_value
            };

        let base_addr =
            if pre {
                if write_back {
                    set_reg_value(*op1, new_base);
                }
                new_base
            } else {
                get_reg_value(*op1)
            };

        let memory_amount = MemoryAmount::from(inst_info.op);
        match memory_amount {
            MemoryAmount::BYTE => {
                if write {
                    let value = get_reg_value(*op0);
                    vmmap[base_addr as usize] = value as u8;
                } else {
                    let value = vmmap[base_addr as usize];
                    set_reg_value(*op0, value as u32);
                }
            }
            MemoryAmount::HALF => {
                let (_, aligned, _) = unsafe { vmmap[base_addr as usize..].align_to_mut::<u16>() };
                if write {
                    let value = get_reg_value(*op0);
                    aligned[0] = value as u16;
                } else {
                    let value = aligned[0];
                    set_reg_value(*op0, value as u32);
                }
            }
            MemoryAmount::WORD => {
                let (_, aligned, _) = unsafe { vmmap[base_addr as usize..].align_to_mut::<u32>() };
                if write {
                    let value = get_reg_value(*op0);
                    aligned[0] = value;
                } else {
                    let value = aligned[0];
                    set_reg_value(*op0, value);
                }
            }
            MemoryAmount::DOUBLE => {
                let (_, aligned, _) = unsafe { vmmap[base_addr as usize..].align_to_mut::<u32>() };
                if write {
                    let value = get_reg_value(*op0);
                    let value1 = get_reg_value(Reg::from(*op0 as u8 + 1));
                    aligned[0] = value;
                    aligned[1] = value1;
                } else {
                    let value = aligned[0];
                    let value1 = aligned[1];
                    set_reg_value(*op0, value);
                    set_reg_value(Reg::from(*op0 as u8 + 1), value1);
                }
            }
        }

        if !pre && write_back {
            set_reg_value(*op1, new_base);
        }
    }
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_read(handler: *const IndirectMemoryHandler, pc: u32) {
    (*handler).handle_request(pc, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_write(handler: *const IndirectMemoryHandler, pc: u32) {
    (*handler).handle_request(pc, true);
}

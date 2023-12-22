use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::{InstInfo, Operand, ShiftValue};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{MemoryAmount, Op, ShiftType};
use crate::logging::debug_println;
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct InstMemHandler {
    cpu_type: CpuType,
    thread_regs: Rc<RefCell<ThreadRegs>>,
    mem_handler: Arc<RwLock<MemHandler>>,
}

impl InstMemHandler {
    pub fn new(
        cpu_type: CpuType,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        mem_handler: Arc<RwLock<MemHandler>>,
    ) -> Self {
        InstMemHandler {
            cpu_type,
            thread_regs,
            mem_handler,
        }
    }

    fn get_inst_info(opcode: u32, thumb: bool) -> InstInfo {
        if thumb {
            let (op, func) = lookup_thumb_opcode(opcode as u16);
            InstInfo::from(&func(opcode as u16, *op))
        } else {
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        }
    }

    fn handle_request(&mut self, opcode: u32, pc: u32, thumb: bool, write: bool) {
        let inst_info = InstMemHandler::get_inst_info(opcode, thumb);

        let pre = match inst_info.op {
            Op::LdrOfip
            | Op::LdrbOfrplr
            | Op::StrOfip
            | Op::StrhOfip
            | Op::StrPrim
            | Op::LdrImm5T
            | Op::LdrhImm5T
            | Op::LdrPcT
            | Op::LdrSpT
            | Op::StrbImm5T
            | Op::StrImm5T
            | Op::StrhImm5T
            | Op::StrSpT => true,
            Op::LdrPtip => false,
            _ => todo!("{:?}", inst_info),
        };

        let write_back = match inst_info.op {
            Op::LdrOfip
            | Op::LdrbOfrplr
            | Op::StrOfip
            | Op::StrhOfip
            | Op::LdrImm5T
            | Op::LdrhImm5T
            | Op::LdrPcT
            | Op::LdrSpT
            | Op::StrbImm5T
            | Op::StrImm5T
            | Op::StrhImm5T
            | Op::StrSpT => false,
            Op::LdrPtip | Op::StrPrim => true,
            _ => todo!("{:?}", inst_info),
        };

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op1 = operands[1].as_reg_no_shift().unwrap();

        let get_reg_value = |regs: &ThreadRegs, reg: Reg| match reg {
            Reg::PC => pc + 4 + 4 * !thumb as u32,
            _ => *regs.get_reg_value(reg),
        };
        let set_reg_value = |regs: &mut ThreadRegs, reg: Reg, value: u32| {
            *regs.get_reg_value_mut(reg) = value;
        };

        let mut thread_regs = self.thread_regs.borrow_mut();

        let new_base = {
            let mut op1_value = get_reg_value(thread_regs.deref(), *op1);

            if operands.len() == 3 {
                let op2_value = match &operands[2] {
                    Operand::Reg { reg, shift } => {
                        let mut op2_value = get_reg_value(thread_regs.deref(), *reg);

                        if let Some(shift) = shift {
                            let (shift_type, shift) = (*shift).into();
                            let shift_value = match shift {
                                ShiftValue::Reg(reg) => get_reg_value(thread_regs.deref(), reg),
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

                op1_value = unsafe { op1_value.unchecked_add(op2_value) };
            }

            op1_value
        };

        let mut base_addr = if pre {
            if write_back {
                set_reg_value(thread_regs.deref_mut(), *op1, new_base);
            }
            new_base
        } else {
            get_reg_value(thread_regs.deref(), *op1)
        };

        if inst_info.op == Op::LdrPcT {
            base_addr &= !0x3;
        }

        let memory_amount = MemoryAmount::from(inst_info.op);
        match memory_amount {
            MemoryAmount::BYTE => {
                if write {
                    let value = get_reg_value(thread_regs.deref(), *op0);
                    self.mem_handler
                        .write()
                        .unwrap()
                        .write(base_addr, value as u8);
                } else {
                    let value = self.mem_handler.read().unwrap().read::<u8>(base_addr);
                    set_reg_value(thread_regs.deref_mut(), *op0, value as u32);
                }
            }
            MemoryAmount::HALF => {
                if write {
                    let value = get_reg_value(thread_regs.deref(), *op0);
                    self.mem_handler
                        .write()
                        .unwrap()
                        .write(base_addr, value as u16);
                } else {
                    let value = self.mem_handler.read().unwrap().read::<u16>(base_addr);

                    if self.cpu_type == CpuType::ARM7 && base_addr & 1 != 0 {
                        todo!()
                    }

                    set_reg_value(thread_regs.deref_mut(), *op0, value as u32);
                }
            }
            MemoryAmount::WORD => {
                if write {
                    let value = get_reg_value(thread_regs.deref(), *op0);
                    self.mem_handler.write().unwrap().write(base_addr, value);
                } else {
                    let value = self.mem_handler.read().unwrap().read(base_addr);

                    if base_addr & 3 != 0 {
                        todo!("{:?} {:x}", self.cpu_type, pc);
                    }

                    set_reg_value(thread_regs.deref_mut(), *op0, value);
                }
            }
            MemoryAmount::DOUBLE => {
                if write {
                    let value = get_reg_value(thread_regs.deref(), *op0);
                    let value1 = get_reg_value(thread_regs.deref(), Reg::from(*op0 as u8 + 1));
                    let mut mem_handler = self.mem_handler.write().unwrap();
                    mem_handler.write(base_addr, value);
                    mem_handler.write(base_addr + 4, value1);
                } else {
                    let mem_handler = self.mem_handler.read().unwrap();
                    let value = mem_handler.read(base_addr);
                    let value1 = mem_handler.read(base_addr);
                    set_reg_value(thread_regs.deref_mut(), *op0, value);
                    set_reg_value(thread_regs.deref_mut(), Reg::from(*op0 as u8 + 1), value1);
                }
            }
        }

        if !pre && write_back {
            set_reg_value(thread_regs.deref_mut(), *op1, new_base);
        }
    }

    fn handle_multiple_request(&mut self, opcode: u32, pc: u32, thumb: bool, write: bool) {
        debug_println!(
            "handle multiple request at {:x} thumb: {} write: {}",
            pc,
            thumb,
            write
        );

        let inst_info = InstMemHandler::get_inst_info(opcode, thumb);

        let mut pre =
            match inst_info.op {
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

        let write_back =
            match inst_info.op {
                Op::Ldmia => false,
                Op::LdmiaW | Op::StmiaW | Op::PushLrT | Op::LdmiaT | Op::PopT => true,
                _ => todo!("{:?}", inst_info),
            };

        let operands = inst_info.operands();

        let op0 = operands[0].as_reg_no_shift().unwrap();
        let mut rlist = RegReserve::from(inst_info.opcode & (0xFF | (!thumb as u32 * 0xFF00)));
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

        let mut thread_regs = self.thread_regs.borrow_mut();

        let start_addr = *thread_regs.get_reg_value(*op0);
        let mut addr = start_addr - (decrement as u32 * rlist.len() as u32 * 4);

        // TODO use batches
        if write {
            let mut mem_handler = self.mem_handler.write().unwrap();
            for reg in rlist {
                addr += pre as u32 * 4;
                let value = *thread_regs.get_reg_value(reg);
                mem_handler.write(addr, value);
                addr += !pre as u32 * 4;
            }
        } else {
            let mem_handler = self.mem_handler.read().unwrap();
            for reg in rlist {
                addr += pre as u32 * 4;
                let value = mem_handler.read(addr);
                *thread_regs.get_reg_value_mut(reg) = value;
                addr += !pre as u32 * 4;
            }
        }

        if write_back {
            *thread_regs.get_reg_value_mut(*op0) = (decrement as u32 * (start_addr - rlist.len() as u32 * 4)) // decrement
                + (!decrement as u32 * addr); // increment
        }
    }
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_read(handler: *mut InstMemHandler, opcode: u32, pc: u32) {
    (*handler).handle_request(opcode, pc, false, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_write(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request(opcode, pc, false, true);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_read_thumb(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request(opcode, pc, true, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_write_thumb(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request(opcode, pc, true, true);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_multiple_read(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request(opcode, pc, false, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_multiple_write(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request(opcode, pc, false, true);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_multiple_read_thumb(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request(opcode, pc, true, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_multiple_write_thumb(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request(opcode, pc, true, true);
}

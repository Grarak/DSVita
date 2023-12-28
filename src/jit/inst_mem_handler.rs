use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::{InstInfo, Operand, ShiftValue};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{MemoryAmount, Op, ShiftType};
use crate::logging::debug_println;
use crate::utils::FastCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::Arc;

pub struct InstMemHandler {
    cpu_type: CpuType,
    thread_regs: Rc<FastCell<ThreadRegs>>,
    mem_handler: Arc<MemHandler>,
}

impl InstMemHandler {
    pub fn new(
        cpu_type: CpuType,
        thread_regs: Rc<FastCell<ThreadRegs>>,
        mem_handler: Arc<MemHandler>,
    ) -> Self {
        InstMemHandler {
            cpu_type,
            thread_regs,
            mem_handler,
        }
    }

    fn get_inst_info<const THUMB: bool>(opcode: u32) -> InstInfo {
        if THUMB {
            let (op, func) = lookup_thumb_opcode(opcode as u16);
            InstInfo::from(&func(opcode as u16, *op))
        } else {
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        }
    }

    fn handle_request<const THUMB: bool, const WRITE: bool>(&mut self, opcode: u32, pc: u32) {
        let inst_info = InstMemHandler::get_inst_info::<THUMB>(opcode);

        let pre =
            match inst_info.op {
                Op::LdrOfip
                | Op::LdrbOfrplr
                | Op::StrOfip
                | Op::StrhOfip
                | Op::StrPrim
                | Op::LdrshRegT
                | Op::LdrbImm5T
                | Op::LdrImm5T
                | Op::LdrhImm5T
                | Op::LdrPcT
                | Op::LdrSpT
                | Op::StrbImm5T
                | Op::StrhRegT
                | Op::StrhImm5T
                | Op::StrRegT
                | Op::StrImm5T
                | Op::StrSpT => true,
                Op::LdrPtip => false,
                _ => todo!("{:?}", inst_info),
            };

        let write_back = match inst_info.op {
            Op::LdrOfip
            | Op::LdrbOfrplr
            | Op::StrOfip
            | Op::StrhOfip
            | Op::LdrshRegT
            | Op::LdrbImm5T
            | Op::LdrImm5T
            | Op::LdrhImm5T
            | Op::LdrPcT
            | Op::LdrSpT
            | Op::StrbImm5T
            | Op::StrhRegT
            | Op::StrhImm5T
            | Op::StrRegT
            | Op::StrImm5T
            | Op::StrSpT => false,
            Op::LdrPtip | Op::StrPrim => true,
            _ => todo!("{:?}", inst_info),
        };

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op1 = operands[1].as_reg_no_shift().unwrap();

        let get_reg_value = |regs: &ThreadRegs, reg: Reg| match reg {
            Reg::PC => pc + 4 + 4 * !THUMB as u32,
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
                if WRITE {
                    let value = get_reg_value(thread_regs.deref(), *op0);
                    self.mem_handler.write(base_addr, value as u8);
                } else {
                    let value = self.mem_handler.read::<u8>(base_addr);
                    set_reg_value(thread_regs.deref_mut(), *op0, value as u32);
                }
            }
            MemoryAmount::HALF => {
                if WRITE {
                    let value = get_reg_value(thread_regs.deref(), *op0);
                    self.mem_handler.write(base_addr, value as u16);
                } else {
                    let value = self.mem_handler.read::<u16>(base_addr);

                    if self.cpu_type == CpuType::ARM7 && base_addr & 1 != 0 {
                        todo!()
                    }

                    if inst_info.op == Op::LdrshRegT {
                        set_reg_value(thread_regs.deref_mut(), *op0, (value as i16) as u32);
                    } else {
                        set_reg_value(thread_regs.deref_mut(), *op0, value as u32);
                    }
                }
            }
            MemoryAmount::WORD => {
                if WRITE {
                    let value = get_reg_value(thread_regs.deref(), *op0);
                    self.mem_handler.write(base_addr, value);
                } else {
                    let value = self.mem_handler.read(base_addr);

                    if base_addr & 3 != 0 {
                        todo!("{:?} {:x}", self.cpu_type, pc);
                    }

                    set_reg_value(thread_regs.deref_mut(), *op0, value);
                }
            }
            MemoryAmount::DOUBLE => {
                if WRITE {
                    let value = get_reg_value(thread_regs.deref(), *op0);
                    let value1 = get_reg_value(thread_regs.deref(), Reg::from(*op0 as u8 + 1));
                    self.mem_handler.write(base_addr, value);
                    self.mem_handler.write(base_addr + 4, value1);
                } else {
                    let value = self.mem_handler.read(base_addr);
                    let value1 = self.mem_handler.read(base_addr);
                    set_reg_value(thread_regs.deref_mut(), *op0, value);
                    set_reg_value(thread_regs.deref_mut(), Reg::from(*op0 as u8 + 1), value1);
                }
            }
        }

        if !pre && write_back {
            set_reg_value(thread_regs.deref_mut(), *op1, new_base);
        }
    }

    fn handle_multiple_request<const THUMB: bool, const WRITE: bool>(
        &mut self,
        opcode: u32,
        pc: u32,
    ) {
        debug_println!(
            "handle multiple request at {:x} thumb: {} write: {}",
            pc,
            THUMB,
            WRITE
        );

        let inst_info = InstMemHandler::get_inst_info::<THUMB>(opcode);

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
        let mut rlist = RegReserve::from(inst_info.opcode & if THUMB { 0xFF } else { 0xFFFF });
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
        if WRITE {
            for reg in rlist {
                addr += pre as u32 * 4;
                let value = *thread_regs.get_reg_value(reg);
                self.mem_handler.write(addr, value);
                addr += !pre as u32 * 4;
            }
        } else {
            for reg in rlist {
                addr += pre as u32 * 4;
                let value = self.mem_handler.read(addr);
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
    (*handler).handle_request::<false, false>(opcode, pc);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_write(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request::<false, true>(opcode, pc);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_read_thumb(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request::<true, false>(opcode, pc);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_write_thumb(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request::<true, true>(opcode, pc);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_multiple_read(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request::<false, false>(opcode, pc);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_multiple_write(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request::<false, true>(opcode, pc);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_multiple_read_thumb(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request::<true, false>(opcode, pc);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn inst_mem_handler_multiple_write_thumb(
    handler: *mut InstMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_multiple_request::<true, true>(opcode, pc);
}

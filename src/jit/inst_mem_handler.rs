use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{MemoryAmount, Op};
use crate::logging::debug_println;
use std::cell::RefCell;
use std::rc::Rc;

pub struct InstMemHandler<const CPU: CpuType> {
    thread_regs: Rc<RefCell<ThreadRegs<CPU>>>,
    mem_handler: Rc<MemHandler<CPU>>,
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

impl<const CPU: CpuType> InstMemHandler<CPU> {
    pub fn new(
        thread_regs: Rc<RefCell<ThreadRegs<CPU>>>,
        mem_handler: Rc<MemHandler<CPU>>,
    ) -> Self {
        InstMemHandler {
            thread_regs,
            mem_handler,
        }
    }

    fn handle_request<const WRITE: bool, const AMOUNT: MemoryAmount>(
        &mut self,
        op0: &mut u32,
        addr: u32,
    ) {
        if WRITE {
            match AMOUNT {
                MemoryAmount::Byte => {
                    self.mem_handler.write(addr, *op0 as u8);
                }
                MemoryAmount::Half => {
                    self.mem_handler.write(addr, *op0 as u16);
                }
                MemoryAmount::Word => {
                    self.mem_handler.write(addr, *op0);
                }
                MemoryAmount::Double => {
                    self.mem_handler.write(addr, *op0);
                    let next_reg =
                        unsafe { (((op0 as *mut _ as u32) + 4) as *mut u32).as_ref().unwrap() };
                    self.mem_handler.write(addr + 4, *next_reg);
                }
            }
        } else {
            match AMOUNT {
                MemoryAmount::Byte => {
                    *op0 = self.mem_handler.read::<u8>(addr) as u32;
                }
                MemoryAmount::Half => {
                    *op0 = self.mem_handler.read::<u16>(addr) as u32;
                }
                MemoryAmount::Word => {
                    *op0 = self.mem_handler.read(addr);
                }
                MemoryAmount::Double => {
                    *op0 = self.mem_handler.read(addr);
                    let next_reg =
                        unsafe { (((op0 as *mut _ as u32) + 4) as *mut u32).as_mut().unwrap() };
                    *next_reg = self.mem_handler.read(addr + 4);
                }
            }
        }
    }

    fn handle_multiple_request<const THUMB: bool, const WRITE: bool>(
        &mut self,
        opcode: u32,
        pc: u32,
        flags: u8,
    ) {
        debug_println!(
            "handle multiple request at {:x} thumb: {} write: {}",
            pc,
            THUMB,
            WRITE
        );

        let inst_info = get_inst_info::<THUMB>(opcode);

        let pre = flags & 1 != 0;
        let write_back = flags & 2 != 0;
        let decrement = flags & 4 != 0;

        let operands = inst_info.operands();

        let op0 = operands[0].as_reg_no_shift().unwrap();
        let mut rlist = RegReserve::from(inst_info.opcode & if THUMB { 0xFF } else { 0xFFFF });
        if inst_info.op == Op::PushLrT {
            rlist += Reg::LR;
        } else if inst_info.op == Op::PopPcT {
            rlist += Reg::PC;
        }

        let mut thread_regs = self.thread_regs.borrow_mut();

        if rlist.len() == 0 {
            todo!()
        }

        if rlist.is_reserved(Reg::PC) || *op0 == Reg::PC {
            *thread_regs.get_reg_value_mut(Reg::PC) = pc + if THUMB { 4 } else { 8 };
        }

        let start_addr = *thread_regs.get_reg_value(*op0);
        let mut addr = start_addr - (decrement as u32 * rlist.len() as u32 * 4);

        if WRITE && CPU == CpuType::ARM7 && write_back && rlist.is_reserved(*op0) {
            todo!()
        }

        // TODO use batches
        for reg in rlist {
            addr += pre as u32 * 4;
            if WRITE {
                let value = *thread_regs.get_reg_value(reg);
                self.mem_handler.write(addr, value);
            } else {
                let value = self.mem_handler.read(addr);
                *thread_regs.get_reg_value_mut(reg) = value;
            }
            addr += !pre as u32 * 4;
        }

        if write_back {
            if !WRITE && CPU == CpuType::ARM9 && rlist.is_reserved(*op0) {
                todo!()
            }

            *thread_regs.get_reg_value_mut(*op0) = (decrement as u32 * (start_addr - rlist.len() as u32 * 4)) // decrement
                + (!decrement as u32 * addr); // increment
        }
    }
}

pub unsafe extern "C" fn inst_mem_handler<
    const CPU: CpuType,
    const WRITE: bool,
    const AMOUNT: MemoryAmount,
>(
    addr: u32,
    op0: *mut u32,
    handler: *mut InstMemHandler<CPU>,
) {
    (*handler).handle_request::<WRITE, AMOUNT>(op0.as_mut().unwrap(), addr);
}

pub unsafe extern "C" fn inst_mem_handler_multiple<
    const CPU: CpuType,
    const THUMB: bool,
    const WRITE: bool,
>(
    handler: *mut InstMemHandler<CPU>,
    opcode: u32,
    pc: u32,
    flags: u8,
) {
    (*handler).handle_multiple_request::<THUMB, WRITE>(opcode, pc, flags);
}

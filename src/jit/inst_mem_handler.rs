use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::MemoryAmount;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[bitsize(32)]
#[derive(FromBits)]
pub struct InstMemMultipleArgs {
    pre: u1,
    write_back: u1,
    decrement: u1,
    op0_reg: u5,
    rlist: u24,
}

pub struct InstMemHandler<const CPU: CpuType> {
    thread_regs: Rc<RefCell<ThreadRegs<CPU>>>,
    mem_handler: Rc<MemHandler<CPU>>,
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
        pc: u32,
        args: u32,
    ) {
        debug_println!(
            "handle multiple request at {:x} thumb: {} write: {}",
            pc,
            THUMB,
            WRITE
        );

        let args = InstMemMultipleArgs::from(args);

        let rlist = RegReserve::from(u32::from(args.rlist()));
        let op0 = Reg::from(u8::from(args.op0_reg()));
        let pre = bool::from(args.pre());
        let write_back = bool::from(args.write_back());
        let decrement = bool::from(args.decrement());

        let mut thread_regs = self.thread_regs.borrow_mut();

        if rlist.len() == 0 {
            todo!()
        }

        if rlist.is_reserved(Reg::PC) || op0 == Reg::PC {
            *thread_regs.get_reg_value_mut(Reg::PC) = pc + if THUMB { 4 } else { 8 };
        }

        let start_addr = if decrement {
            *thread_regs.get_reg_value(op0) - ((rlist.len() as u32) << 2)
        } else {
            *thread_regs.get_reg_value(op0)
        };
        let mut addr = start_addr;

        if WRITE && CPU == CpuType::ARM7 && write_back && rlist.is_reserved(op0) {
            todo!()
        }

        for i in Reg::R0 as u8..Reg::CPSR as u8 {
            let reg = Reg::from(i);
            if rlist.is_reserved(reg) {
                addr += (pre as u32) << 2;
                if WRITE {
                    let value = *thread_regs.get_reg_value(reg);
                    self.mem_handler.write(addr, value);
                } else {
                    let value = self.mem_handler.read(addr);
                    *thread_regs.get_reg_value_mut(reg) = value;
                }
                addr += (!pre as u32) << 2;
            }
        }

        if write_back {
            if !WRITE && CPU == CpuType::ARM9 && rlist.is_reserved(op0) {
                todo!()
            }

            *thread_regs.get_reg_value_mut(op0) = if decrement { start_addr } else { addr }
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
    (*handler).handle_request::<WRITE, AMOUNT>(op0.as_mut().unwrap_unchecked(), addr);
}

pub unsafe extern "C" fn inst_mem_handler_multiple<
    const CPU: CpuType,
    const THUMB: bool,
    const WRITE: bool,
>(
    handler: *mut InstMemHandler<CPU>,
    pc: u32,
    args: u32,
) {
    (*handler).handle_multiple_request::<THUMB, WRITE>(pc, args);
}

use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::MemoryAmount;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::cell::RefCell;
use std::ops::DerefMut;
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

    fn handle_request<const WRITE: bool, const AMOUNT: MemoryAmount, const SIGNED: bool>(
        &self,
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
                        unsafe { (op0 as *mut u32).offset(1).as_ref().unwrap_unchecked() };
                    self.mem_handler.write(addr + 4, *next_reg);
                }
            }
        } else {
            match AMOUNT {
                MemoryAmount::Byte => {
                    if SIGNED {
                        *op0 = self.mem_handler.read::<u8>(addr) as i8 as i32 as u32;
                    } else {
                        *op0 = self.mem_handler.read::<u8>(addr) as u32;
                    }
                }
                MemoryAmount::Half => {
                    if SIGNED {
                        *op0 = self.mem_handler.read::<u16>(addr) as i16 as i32 as u32;
                    } else {
                        *op0 = self.mem_handler.read::<u16>(addr) as u32;
                    }
                }
                MemoryAmount::Word => {
                    if (addr & 0x3) == 0 {
                        *op0 = self.mem_handler.read(addr);
                    } else {
                        let value = self.mem_handler.read::<u32>(addr);
                        let shift = (addr & 0x3) << 3;
                        let value = (value << (32 - shift)) | (value >> shift);
                        *op0 = value;
                    }
                }
                MemoryAmount::Double => {
                    *op0 = self.mem_handler.read(addr);
                    let next_reg =
                        unsafe { (op0 as *mut u32).offset(1).as_mut().unwrap_unchecked() };
                    *next_reg = self.mem_handler.read(addr + 4);
                }
            }
        }
    }

    fn handle_multiple_request<const THUMB: bool, const WRITE: bool, const USER: bool>(
        &self,
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
        let thread_regs = thread_regs.deref_mut();

        if rlist.len() == 0 {
            if WRITE {
                *thread_regs.get_reg_value_mut(op0) -= 0x40;
            } else {
                *thread_regs.get_reg_value_mut(op0) += 0x40;
            }
            if CPU == CpuType::ARM7 {
                todo!()
            }
            return;
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

        if write_back
            && (!WRITE
                || (CPU == CpuType::ARM7
                    && (rlist.0 & ((1 << (op0 as u8 + 1)) - 1)) > (1 << op0 as u8)))
        {
            if decrement {
                *thread_regs.get_reg_value_mut(op0) = addr;
            } else {
                *thread_regs.get_reg_value_mut(op0) = addr + ((rlist.len() as u32) << 2);
            }
        }

        let get_reg_fun = if USER && !rlist.is_reserved(Reg::PC) {
            ThreadRegs::<CPU>::get_reg_usr_value_mut
        } else {
            ThreadRegs::<CPU>::get_reg_value_mut
        };
        for i in Reg::R0 as u8..Reg::CPSR as u8 {
            let reg = Reg::from(i);
            if rlist.is_reserved(reg) {
                addr += (pre as u32) << 2;
                if WRITE {
                    let value = get_reg_fun(thread_regs, reg);
                    self.mem_handler.write(addr, *value);
                } else {
                    let value = self.mem_handler.read(addr);
                    *get_reg_fun(thread_regs, reg) = value;
                }
                addr += (!pre as u32) << 2;
            }
        }

        if write_back
            && (WRITE
                || (CPU == CpuType::ARM9
                    && ((rlist.0 & !((1 << (op0 as u8 + 1)) - 1)) != 0
                        || (rlist.0 == (1 << op0 as u8)))))
        {
            *thread_regs.get_reg_value_mut(op0) = if decrement { start_addr } else { addr }
        }

        if USER && rlist.is_reserved(Reg::PC) {
            todo!()
        }
    }

    fn handle_swp_request<const AMOUNT: MemoryAmount>(&self, op0: &mut u32, value: u32, addr: u32) {
        if AMOUNT == MemoryAmount::Byte {
            *op0 = self.mem_handler.read::<u8>(addr) as u32;
            self.mem_handler.write(addr, (value & 0xFF) as u8);
        } else {
            *op0 = self.mem_handler.read(addr);
            self.mem_handler.write(addr, value);
        }
    }
}

pub unsafe extern "C" fn inst_mem_handler<
    const CPU: CpuType,
    const WRITE: bool,
    const AMOUNT: MemoryAmount,
    const SIGNED: bool,
>(
    addr: u32,
    op0: *mut u32,
    handler: *const InstMemHandler<CPU>,
) {
    (*handler).handle_request::<WRITE, AMOUNT, SIGNED>(op0.as_mut().unwrap_unchecked(), addr);
}

pub unsafe extern "C" fn inst_mem_handler_multiple<
    const CPU: CpuType,
    const THUMB: bool,
    const WRITE: bool,
>(
    handler: *const InstMemHandler<CPU>,
    pc: u32,
    args: u32,
) {
    (*handler).handle_multiple_request::<THUMB, WRITE, false>(pc, args);
}

pub unsafe extern "C" fn inst_mem_handler_multiple_user<const CPU: CpuType, const WRITE: bool>(
    handler: *const InstMemHandler<CPU>,
    pc: u32,
    args: u32,
) {
    (*handler).handle_multiple_request::<false, WRITE, true>(pc, args);
}

pub unsafe extern "C" fn inst_mem_handler_swp<const CPU: CpuType, const AMOUNT: MemoryAmount>(
    handler: *const InstMemHandler<CPU>,
    op0: *mut u32,
    value: u32,
    addr: u32,
) {
    (*handler).handle_swp_request::<AMOUNT>(op0.as_mut().unwrap_unchecked(), value, addr);
}

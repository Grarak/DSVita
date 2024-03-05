use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::MemoryAmount;
use crate::logging::debug_println;
use std::cell::RefCell;
use std::rc::Rc;

pub struct InstMemHandler<const CPU: CpuType> {
    thread_regs: *mut ThreadRegs<CPU>,
    mem_handler: *const MemHandler<CPU>,
}

impl<const CPU: CpuType> InstMemHandler<CPU> {
    pub fn new(
        thread_regs: Rc<RefCell<ThreadRegs<CPU>>>,
        mem_handler: Rc<MemHandler<CPU>>,
    ) -> Self {
        InstMemHandler {
            thread_regs: thread_regs.as_ptr(),
            mem_handler: mem_handler.as_ref(),
        }
    }

    fn handle_request<const WRITE: bool, const AMOUNT: MemoryAmount, const SIGNED: bool>(
        &self,
        op0: &mut u32,
        addr: u32,
    ) {
        let mem_handler = unsafe { self.mem_handler.as_ref().unwrap_unchecked() };
        if WRITE {
            match AMOUNT {
                MemoryAmount::Byte => {
                    mem_handler.write(addr, *op0 as u8);
                }
                MemoryAmount::Half => {
                    mem_handler.write(addr, *op0 as u16);
                }
                MemoryAmount::Word => {
                    mem_handler.write(addr, *op0);
                }
                MemoryAmount::Double => {
                    mem_handler.write(addr, *op0);
                    let next_reg =
                        unsafe { (op0 as *mut u32).offset(1).as_ref().unwrap_unchecked() };
                    mem_handler.write(addr + 4, *next_reg);
                }
            }
        } else {
            match AMOUNT {
                MemoryAmount::Byte => {
                    if SIGNED {
                        *op0 = mem_handler.read::<u8>(addr) as i8 as i32 as u32;
                    } else {
                        *op0 = mem_handler.read::<u8>(addr) as u32;
                    }
                }
                MemoryAmount::Half => {
                    if SIGNED {
                        *op0 = mem_handler.read::<u16>(addr) as i16 as i32 as u32;
                    } else {
                        *op0 = mem_handler.read::<u16>(addr) as u32;
                    }
                }
                MemoryAmount::Word => {
                    if (addr & 0x3) == 0 {
                        *op0 = mem_handler.read(addr);
                    } else {
                        let value = mem_handler.read::<u32>(addr);
                        let shift = (addr & 0x3) << 3;
                        let value = (value << (32 - shift)) | (value >> shift);
                        *op0 = value;
                    }
                }
                MemoryAmount::Double => {
                    *op0 = mem_handler.read(addr);
                    let next_reg =
                        unsafe { (op0 as *mut u32).offset(1).as_mut().unwrap_unchecked() };
                    *next_reg = mem_handler.read(addr + 4);
                }
            }
        }
    }

    fn handle_multiple_request<
        const THUMB: bool,
        const WRITE: bool,
        const USER: bool,
        const PRE: bool,
        const WRITE_BACK: bool,
        const DECREMENT: bool,
    >(
        &self,
        pc: u32,
        rlist: u16,
        op0: u8,
    ) {
        debug_println!(
            "handle multiple request at {:x} thumb: {} write: {}",
            pc,
            THUMB,
            WRITE
        );

        let mem_handler = unsafe { self.mem_handler.as_ref().unwrap_unchecked() };
        let thread_regs = unsafe { self.thread_regs.as_mut().unwrap_unchecked() };

        let rlist = RegReserve::from(rlist as u32);
        let op0 = Reg::from(op0);

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

        let start_addr = if DECREMENT {
            *thread_regs.get_reg_value(op0) - ((rlist.len() as u32) << 2)
        } else {
            *thread_regs.get_reg_value(op0)
        };
        let mut addr = start_addr;

        if WRITE_BACK
            && (!WRITE
                || (CPU == CpuType::ARM7
                    && (rlist.0 & ((1 << (op0 as u8 + 1)) - 1)) > (1 << op0 as u8)))
        {
            if DECREMENT {
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
                if PRE {
                    addr += 4;
                }
                if WRITE {
                    let value = get_reg_fun(thread_regs, reg);
                    mem_handler.write(addr, *value);
                } else {
                    let value = mem_handler.read(addr);
                    *get_reg_fun(thread_regs, reg) = value;
                }
                if !PRE {
                    addr += 4;
                }
            }
        }

        if WRITE_BACK
            && (WRITE
                || (CPU == CpuType::ARM9
                    && ((rlist.0 & !((1 << (op0 as u8 + 1)) - 1)) != 0
                        || (rlist.0 == (1 << op0 as u8)))))
        {
            *thread_regs.get_reg_value_mut(op0) = if DECREMENT { start_addr } else { addr }
        }

        if USER && rlist.is_reserved(Reg::PC) {
            todo!()
        }
    }

    fn handle_swp_request<const AMOUNT: MemoryAmount>(&self, regs: u32) {
        let mem_handler = unsafe { self.mem_handler.as_ref().unwrap_unchecked() };
        let thread_regs = unsafe { self.thread_regs.as_mut().unwrap_unchecked() };

        let op0 = Reg::from((regs & 0xFF) as u8);
        let op1 = Reg::from(((regs >> 8) & 0xFF) as u8);
        let op2 = Reg::from(((regs >> 16) & 0xFF) as u8);

        let value = *thread_regs.get_reg_value(op1);
        let addr = *thread_regs.get_reg_value(op2);
        let op0 = thread_regs.get_reg_value_mut(op0);

        if AMOUNT == MemoryAmount::Byte {
            *op0 = mem_handler.read::<u8>(addr) as u32;
            mem_handler.write(addr, (value & 0xFF) as u8);
        } else {
            *op0 = mem_handler.read(addr);
            mem_handler.write(addr, value);
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
    pc: u32,
    asm: *const JitAsm<CPU>,
) {
    (*asm)
        .inst_mem_handler
        .handle_request::<WRITE, AMOUNT, SIGNED>(op0.as_mut().unwrap_unchecked(), addr);
    // ARM7 can halt the CPU with an IO port write
    if CPU == CpuType::ARM7 && WRITE && (*asm).cpu_regs.as_ref().is_halted() {
        todo!()
    }
}

pub unsafe extern "C" fn inst_mem_handler_multiple<
    const CPU: CpuType,
    const THUMB: bool,
    const WRITE: bool,
    const USER: bool,
    const PRE: bool,
    const WRITE_BACK: bool,
    const DECREMENT: bool,
>(
    asm: *const JitAsm<CPU>,
    pc: u32,
    rlist: u16,
    op0: u8,
) {
    (*asm)
        .inst_mem_handler
        .handle_multiple_request::<THUMB, WRITE, USER, PRE, WRITE_BACK, DECREMENT>(pc, rlist, op0);
    // ARM7 can halt the CPU with an IO port write
    if CPU == CpuType::ARM7 && WRITE && (*asm).cpu_regs.as_ref().is_halted() {
        todo!()
    }
}

pub unsafe extern "C" fn inst_mem_handler_swp<const CPU: CpuType, const AMOUNT: MemoryAmount>(
    asm: *const JitAsm<CPU>,
    regs: u32,
    pc: u32,
) {
    (*asm).inst_mem_handler.handle_swp_request::<AMOUNT>(regs);
    // ARM7 can halt the CPU with an IO port write
    if CPU == CpuType::ARM7 && (*asm).cpu_regs.as_ref().is_halted() {
        todo!()
    }
}

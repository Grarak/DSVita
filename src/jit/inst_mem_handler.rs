use crate::hle::hle::{get_regs_mut, Hle};
use crate::hle::CpuType;
use crate::jit::MemoryAmount;
use std::arch::asm;
use std::hint::unreachable_unchecked;
use std::intrinsics::unlikely;

mod handler {
    use crate::hle::hle::{get_regs, get_regs_mut, Hle};
    use crate::hle::thread_regs::ThreadRegs;
    use crate::hle::CpuType;
    use crate::jit::reg::{Reg, RegReserve};
    use crate::jit::MemoryAmount;
    use crate::logging::debug_println;
    use std::intrinsics::{likely, unlikely};

    #[inline(never)]
    pub fn handle_request<
        const CPU: CpuType,
        const WRITE: bool,
        const AMOUNT: MemoryAmount,
        const SIGNED: bool,
    >(
        op0: &mut u32,
        addr: u32,
        hle: &mut Hle,
    ) {
        if WRITE {
            match AMOUNT {
                MemoryAmount::Byte => {
                    hle.mem_write::<CPU, _>(addr, *op0 as u8);
                }
                MemoryAmount::Half => {
                    hle.mem_write::<CPU, _>(addr, *op0 as u16);
                }
                MemoryAmount::Word => {
                    hle.mem_write::<CPU, _>(addr, *op0);
                }
                MemoryAmount::Double => {
                    hle.mem_write::<CPU, _>(addr, *op0);
                    let next_reg =
                        unsafe { (op0 as *mut u32).offset(1).as_ref().unwrap_unchecked() };
                    hle.mem_write::<CPU, _>(addr + 4, *next_reg);
                }
            }
        } else {
            match AMOUNT {
                MemoryAmount::Byte => {
                    if SIGNED {
                        *op0 = hle.mem_read::<CPU, u8>(addr) as i8 as i32 as u32;
                    } else {
                        *op0 = hle.mem_read::<CPU, u8>(addr) as u32;
                    }
                }
                MemoryAmount::Half => {
                    if SIGNED {
                        *op0 = hle.mem_read::<CPU, u16>(addr) as i16 as i32 as u32;
                    } else {
                        *op0 = hle.mem_read::<CPU, u16>(addr) as u32;
                    }
                }
                MemoryAmount::Word => {
                    *op0 = if likely(addr & 0x3 == 0) {
                        hle.mem_read::<CPU, u32>(addr)
                    } else {
                        let value = hle.mem_read::<CPU, u32>(addr);
                        let shift = (addr & 0x3) << 3;
                        (value << (32 - shift)) | (value >> shift)
                    };
                }
                MemoryAmount::Double => {
                    *op0 = hle.mem_read::<CPU, u32>(addr);
                    let next_reg =
                        unsafe { (op0 as *mut u32).offset(1).as_mut().unwrap_unchecked() };
                    *next_reg = hle.mem_read::<CPU, u32>(addr + 4);
                }
            }
        }
    }

    #[inline(never)]
    pub fn handle_multiple_request<
        const CPU: CpuType,
        const THUMB: bool,
        const WRITE: bool,
        const USER: bool,
        const PRE: bool,
        const WRITE_BACK: bool,
        const DECREMENT: bool,
    >(
        pc: u32,
        rlist: u16,
        op0: u8,
        hle: &mut Hle,
    ) {
        debug_println!(
            "handle multiple request at {:x} thumb: {} write: {}",
            pc,
            THUMB,
            WRITE
        );

        let rlist = RegReserve::from(rlist as u32);
        let op0 = Reg::from(op0);

        if unlikely(rlist.len() == 0) {
            if WRITE {
                *get_regs_mut!(hle, CPU).get_reg_mut(op0) -= 0x40;
            } else {
                *get_regs_mut!(hle, CPU).get_reg_mut(op0) += 0x40;
            }
            if CPU == CpuType::ARM7 {
                todo!()
            }
            return;
        }

        if unlikely(rlist.is_reserved(Reg::PC) || op0 == Reg::PC) {
            *get_regs_mut!(hle, CPU).get_reg_mut(Reg::PC) = pc + if THUMB { 4 } else { 8 };
        }

        let start_addr = if DECREMENT {
            *get_regs!(hle, CPU).get_reg(op0) - ((rlist.len() as u32) << 2)
        } else {
            *get_regs!(hle, CPU).get_reg(op0)
        };
        let mut addr = start_addr;

        if WRITE_BACK
            && (!WRITE
                || (CPU == CpuType::ARM7
                    && unlikely((rlist.0 & ((1 << (op0 as u8 + 1)) - 1)) > (1 << op0 as u8))))
        {
            if DECREMENT {
                *get_regs_mut!(hle, CPU).get_reg_mut(op0) = addr;
            } else {
                *get_regs_mut!(hle, CPU).get_reg_mut(op0) = addr + ((rlist.len() as u32) << 2);
            }
        }

        let get_reg_fun = if USER && likely(!rlist.is_reserved(Reg::PC)) {
            ThreadRegs::get_reg_usr_mut
        } else {
            ThreadRegs::get_reg_mut
        };
        for i in Reg::R0 as u8..Reg::CPSR as u8 {
            let reg = Reg::from(i);
            if rlist.is_reserved(reg) {
                if PRE {
                    addr += 4;
                }
                if WRITE {
                    let value = *get_reg_fun(get_regs_mut!(hle, CPU), reg);
                    hle.mem_write::<CPU, _>(addr, value);
                } else {
                    let value = hle.mem_read::<CPU, _>(addr);
                    *get_reg_fun(get_regs_mut!(hle, CPU), reg) = value;
                }
                if !PRE {
                    addr += 4;
                }
            }
        }

        if WRITE_BACK
            && (WRITE
                || (CPU == CpuType::ARM9
                    && unlikely(
                        (rlist.0 & !((1 << (op0 as u8 + 1)) - 1)) != 0
                            || (rlist.0 == (1 << op0 as u8)),
                    )))
        {
            *get_regs_mut!(hle, CPU).get_reg_mut(op0) = if DECREMENT { start_addr } else { addr }
        }

        if USER && unlikely(rlist.is_reserved(Reg::PC)) {
            todo!()
        }
    }

    #[inline(never)]
    pub fn handle_swp_request<const CPU: CpuType, const AMOUNT: MemoryAmount>(
        regs: u32,
        hle: &mut Hle,
    ) {
        let op0 = Reg::from((regs & 0xFF) as u8);
        let op1 = Reg::from(((regs >> 8) & 0xFF) as u8);
        let op2 = Reg::from(((regs >> 16) & 0xFF) as u8);

        let value = *get_regs!(hle, CPU).get_reg(op1);
        let addr = *get_regs!(hle, CPU).get_reg(op2);

        if AMOUNT == MemoryAmount::Byte {
            *get_regs_mut!(hle, CPU).get_reg_mut(op0) = hle.mem_read::<CPU, u8>(addr) as u32;
            hle.mem_write::<CPU, _>(addr, (value & 0xFF) as u8);
        } else {
            *get_regs_mut!(hle, CPU).get_reg_mut(op0) = hle.mem_read::<CPU, _>(addr);
            hle.mem_write::<CPU, _>(addr, value);
        }
    }
}
use crate::jit::jit_asm::JitAsm;
use handler::*;

pub unsafe extern "C" fn test() {
    println!("Some test")
}

pub unsafe extern "C" fn inst_mem_handler<
    const CPU: CpuType,
    const THUMB: bool,
    const WRITE: bool,
    const AMOUNT: MemoryAmount,
    const SIGNED: bool,
>(
    addr: u32,
    op0: *mut u32,
    pc: u32,
    asm: *mut JitAsm<CPU>,
) {
    let asm = asm.as_mut().unwrap_unchecked();
    handle_request::<CPU, WRITE, AMOUNT, SIGNED>(op0.as_mut().unwrap_unchecked(), addr, asm.hle);
    if WRITE && unlikely(asm.hle.mem.breakout_imm) {
        asm.guest_branch_out_pc = pc;
        get_regs_mut!(asm.hle, CPU).pc = pc + if THUMB { 2 } else { 4 };
        asm.hle.mem.breakout_imm = false;
        if THUMB {
            asm!("bx {}", in(reg) asm.breakout_skip_save_regs_thumb_addr);
        } else {
            asm!("bx {}", in(reg) asm.breakout_skip_save_regs_addr);
        }
        unreachable_unchecked();
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
    pc: u32,
    rlist: u16,
    op0: u8,
    hle: *mut Hle,
) {
    handle_multiple_request::<CPU, THUMB, WRITE, USER, PRE, WRITE_BACK, DECREMENT>(
        pc,
        rlist,
        op0,
        hle.as_mut().unwrap_unchecked(),
    );
    if WRITE && unlikely((*hle).mem.breakout_imm) {
        get_regs_mut!(*hle, CPU).pc = pc + if THUMB { 2 } else { 4 };
        todo!()
    }
}

pub unsafe extern "C" fn inst_mem_handler_swp<const CPU: CpuType, const AMOUNT: MemoryAmount>(
    regs: u32,
    pc: u32,
    hle: *mut Hle,
) {
    handle_swp_request::<CPU, AMOUNT>(regs, hle.as_mut().unwrap_unchecked());
    if unlikely((*hle).mem.breakout_imm) {
        get_regs_mut!(*hle, CPU).pc = pc + 4;
        todo!()
    }
}

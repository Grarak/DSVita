use crate::core::emu::get_mem;
use crate::core::CpuType;
use crate::get_jit_asm_ptr;
use crate::jit::reg::Reg;
use crate::jit::MemoryAmount;
use handler::*;
use std::intrinsics::unlikely;

mod handler {
    use crate::core::emu::{get_mem_mut, get_regs_mut, Emu};
    use crate::core::thread_regs::ThreadRegs;
    use crate::core::CpuType;
    use crate::jit::reg::{Reg, RegReserve};
    use crate::jit::MemoryAmount;
    use crate::logging::debug_println;
    use std::hint::unreachable_unchecked;
    use std::intrinsics::{likely, unlikely};

    pub fn handle_request<const CPU: CpuType, const WRITE: bool, const AMOUNT: MemoryAmount, const SIGNED: bool>(op0: &mut u32, addr: u32, emu: &mut Emu) {
        if WRITE {
            match AMOUNT {
                MemoryAmount::Byte => {
                    emu.mem_write::<CPU, _>(addr, *op0 as u8);
                }
                MemoryAmount::Half => {
                    emu.mem_write::<CPU, _>(addr, *op0 as u16);
                }
                MemoryAmount::Word => {
                    emu.mem_write::<CPU, _>(addr, *op0);
                }
                MemoryAmount::Double => {
                    emu.mem_write::<CPU, _>(addr, *op0);
                    let next_reg = unsafe { (op0 as *mut u32).offset(1).as_ref().unwrap_unchecked() };
                    emu.mem_write::<CPU, _>(addr + 4, *next_reg);
                }
            }
        } else {
            match AMOUNT {
                MemoryAmount::Byte => {
                    if SIGNED {
                        *op0 = emu.mem_read_with_options::<CPU, true, u8>(addr) as i8 as i32 as u32;
                    } else {
                        *op0 = emu.mem_read_with_options::<CPU, true, u8>(addr) as u32;
                    }
                }
                MemoryAmount::Half => {
                    if SIGNED {
                        *op0 = emu.mem_read_with_options::<CPU, true, u16>(addr) as i16 as i32 as u32;
                    } else {
                        *op0 = emu.mem_read_with_options::<CPU, true, u16>(addr) as u32;
                    }
                }
                MemoryAmount::Word => {
                    let value = emu.mem_read_with_options::<CPU, true, u32>(addr);
                    let shift = (addr & 0x3) << 3;
                    *op0 = value.wrapping_shl(32 - shift) | (value >> shift)
                }
                MemoryAmount::Double => {
                    *op0 = emu.mem_read_with_options::<CPU, true, u32>(addr);
                    let next_reg = unsafe { (op0 as *mut u32).offset(1).as_mut().unwrap_unchecked() };
                    *next_reg = emu.mem_read_with_options::<CPU, true, u32>(addr + 4);
                }
            }
        }
    }

    pub fn handle_multiple_request<const CPU: CpuType, const THUMB: bool, const WRITE: bool, const USER: bool, const PRE: bool, const WRITE_BACK: bool, const DECREMENT: bool>(
        pc: u32,
        rlist: u16,
        op0: u8,
        emu: &mut Emu,
    ) {
        debug_println!("handle multiple request at {:x} thumb: {} write: {}", pc, THUMB, WRITE);

        let rlist = RegReserve::from(rlist as u32);
        let op0 = Reg::from(op0);

        let regs = get_regs_mut!(emu, CPU);

        if unlikely(rlist.is_empty()) {
            if WRITE {
                *regs.get_reg_mut(op0) -= 0x40;
            } else {
                *regs.get_reg_mut(op0) += 0x40;
            }
            if CPU == CpuType::ARM7 {
                unsafe { unreachable_unchecked() }
            }
            return;
        }

        if WRITE && unlikely(rlist.is_reserved(Reg::PC) || op0 == Reg::PC) {
            *regs.get_reg_mut(Reg::PC) = pc + if THUMB { 4 } else { 8 };
        }

        let start_addr = if DECREMENT { *regs.get_reg(op0) - ((rlist.len() as u32) << 2) } else { *regs.get_reg(op0) };
        let addr = start_addr;

        if WRITE_BACK && (!WRITE || (CPU == CpuType::ARM7 && unlikely((rlist.0 & ((1 << (op0 as u8 + 1)) - 1)) > (1 << op0 as u8)))) {
            if DECREMENT {
                *regs.get_reg_mut(op0) = addr;
            } else {
                *regs.get_reg_mut(op0) = addr + ((rlist.len() as u32) << 2);
            }
        }

        let get_reg_fun = if USER && likely(!rlist.is_reserved(Reg::PC)) {
            ThreadRegs::get_reg_usr_mut
        } else {
            ThreadRegs::get_reg_mut
        };

        let mem_addr = if PRE { addr + 4 } else { addr };

        let mut rlist_iter = rlist.into_iter();
        if WRITE {
            get_mem_mut!(emu).write_multiple::<CPU, u32, _>(mem_addr, emu, rlist.len(), || {
                let reg = unsafe { rlist_iter.next().unwrap_unchecked() };
                *get_reg_fun(regs, reg)
            });
        } else {
            get_mem_mut!(emu).read_multiple::<CPU, u32, _>(mem_addr, emu, rlist.len(), |value| {
                let reg = unsafe { rlist_iter.next().unwrap_unchecked() };
                *get_reg_fun(regs, reg) = value;
            });
        }

        if WRITE_BACK && (WRITE || (CPU == CpuType::ARM9 && unlikely((rlist.0 & !((1 << (op0 as u8 + 1)) - 1)) != 0 || (rlist.0 == (1 << op0 as u8))))) {
            *regs.get_reg_mut(op0) = if DECREMENT { start_addr } else { addr + (rlist.len() << 2) as u32 }
        }

        if USER && unlikely(rlist.is_reserved(Reg::PC)) {
            unsafe { unreachable_unchecked() }
        }
    }

    pub fn handle_swp_request<const CPU: CpuType, const AMOUNT: MemoryAmount>(op0: Reg, value: u32, addr: u32, emu: &mut Emu) {
        if AMOUNT == MemoryAmount::Byte {
            *get_regs_mut!(emu, CPU).get_reg_mut(op0) = emu.mem_read::<CPU, u8>(addr) as u32;
            emu.mem_write::<CPU, _>(addr, (value & 0xFF) as u8);
        } else {
            *get_regs_mut!(emu, CPU).get_reg_mut(op0) = emu.mem_read::<CPU, _>(addr);
            emu.mem_write::<CPU, _>(addr, value);
        }
    }
}

macro_rules! imm_breakout {
    ($asm:expr, $pc:expr, $thumb:expr, $total_cycles:expr) => {{
        crate::logging::debug_println!("immediate breakout");
        if crate::IS_DEBUG {
            $asm.runtime_data.branch_out_pc = $pc;
        }
        $asm.runtime_data.accumulated_cycles += $total_cycles - $asm.runtime_data.pre_cycle_count_sum;
        crate::core::emu::get_regs_mut!($asm.emu, CPU).pc = $pc + if $thumb { 3 } else { 4 };
        crate::core::emu::get_mem_mut!($asm.emu).breakout_imm = false;
        crate::jit::jit_asm_common_funs::exit_guest_context!($asm);
    }};
}
pub(super) use imm_breakout;

pub unsafe extern "C" fn inst_mem_handler<const CPU: CpuType, const THUMB: bool, const WRITE: bool, const AMOUNT: MemoryAmount, const SIGNED: bool>(
    addr: u32,
    op0: *mut u32,
    pc: u32,
    total_cycles: u16,
) {
    let asm = get_jit_asm_ptr::<CPU>();
    handle_request::<CPU, WRITE, AMOUNT, SIGNED>(op0.as_mut().unwrap_unchecked(), addr, (*asm).emu);
    if WRITE && unlikely(get_mem!((*asm).emu).breakout_imm) {
        imm_breakout!((*asm), pc, THUMB, total_cycles);
    }
}

pub unsafe extern "C" fn inst_mem_handler_multiple<const CPU: CpuType, const THUMB: bool, const WRITE: bool, const USER: bool, const PRE: bool, const WRITE_BACK: bool, const DECREMENT: bool>(
    op0_rlist: u32,
    pc: u32,
    total_cycles: u16,
) {
    let asm = get_jit_asm_ptr::<CPU>();
    handle_multiple_request::<CPU, THUMB, WRITE, USER, PRE, WRITE_BACK, DECREMENT>(pc, (op0_rlist & 0xFFFF) as u16, (op0_rlist >> 16) as u8, (*asm).emu);
    if WRITE && unlikely(get_mem!((*asm).emu).breakout_imm) {
        imm_breakout!((*asm), pc, THUMB, total_cycles);
    }
}

pub unsafe extern "C" fn inst_mem_handler_swp<const CPU: CpuType, const AMOUNT: MemoryAmount>(value: u32, addr: u32, pc: u32, op0_total_cycles: u32) {
    let asm = get_jit_asm_ptr::<CPU>();
    handle_swp_request::<CPU, AMOUNT>(Reg::from((op0_total_cycles >> 16) as u8), value, addr, (*asm).emu);
    if unlikely(get_mem!((*asm).emu).breakout_imm) {
        imm_breakout!((*asm), pc, false, op0_total_cycles as u16);
    }
}

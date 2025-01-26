use crate::core::emu::get_mem;
use crate::core::CpuType;
use crate::get_jit_asm_ptr;
use crate::jit::reg::Reg;
use crate::jit::MemoryAmount;
use handler::*;
use std::intrinsics::unlikely;

mod handler {
    use crate::core::emu::{get_mem_mut, get_regs, get_regs_mut, Emu};
    use crate::core::thread_regs::ThreadRegs;
    use crate::core::CpuType;
    use crate::jit::reg::{Reg, RegReserve};
    use crate::jit::MemoryAmount;
    use crate::logging::debug_println;
    use std::hint::unreachable_unchecked;
    use std::intrinsics::unlikely;

    pub fn handle_request_write<const CPU: CpuType, const AMOUNT: MemoryAmount>(op0: u32, addr: u32, emu: &mut Emu, op0_reg: Reg) {
        match AMOUNT {
            MemoryAmount::Byte => {
                emu.mem_write::<CPU, _>(addr, op0 as u8);
            }
            MemoryAmount::Half => {
                emu.mem_write::<CPU, _>(addr, op0 as u16);
            }
            MemoryAmount::Word => {
                emu.mem_write::<CPU, _>(addr, op0);
            }
            MemoryAmount::Double => {
                emu.mem_write::<CPU, _>(addr, op0);
                emu.mem_write::<CPU, _>(addr + 4, *get_regs!(emu, CPU).get_reg(Reg::from(op0_reg as u8 + 1)));
            }
        }
    }

    pub fn handle_request_read<const CPU: CpuType, const AMOUNT: MemoryAmount, const SIGNED: bool>(op0: &mut u32, addr: u32, emu: &mut Emu) {
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

    fn get_reg_usr_mut<const FIQ_MODE: bool>(regs: &mut ThreadRegs, reg: Reg) -> &mut u32 {
        if FIQ_MODE {
            regs.get_reg_usr_mut(reg)
        } else if reg == Reg::SP || reg == Reg::LR {
            ThreadRegs::get_reg_usr_mut(regs, reg)
        } else {
            ThreadRegs::get_reg_mut(regs, reg)
        }
    }

    pub fn handle_multiple_request<const CPU: CpuType, const WRITE: bool, const USER: bool, const PRE: bool, const WRITE_BACK: bool, const DECREMENT: bool>(
        pc: u32,
        rlist: u16,
        op0: u8,
        emu: &mut Emu,
    ) {
        let is_thumb = pc & 1 == 1;
        let pc = pc & !1;
        debug_println!("handle multiple request at {pc:x} thumb: {is_thumb} write: {WRITE}");

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
            let pc_offset = 4 << (!is_thumb as u8);
            *regs.get_reg_mut(Reg::PC) = pc + pc_offset;
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

        let mem_addr = if PRE { addr + 4 } else { addr };

        let get_reg_mut = if USER && !rlist.is_reserved(Reg::PC) {
            if get_regs!(emu, CPU).is_fiq_mode() {
                get_reg_usr_mut::<true>
            } else {
                get_reg_usr_mut::<false>
            }
        } else {
            ThreadRegs::get_reg_mut
        };

        let mut rlist_iter = rlist.into_iter();
        if WRITE {
            get_mem_mut!(emu).write_multiple::<CPU, u32, _>(mem_addr, emu, rlist.len(), || {
                let reg = unsafe { rlist_iter.next().unwrap_unchecked() };
                *get_reg_mut(regs, reg)
            });
        } else {
            get_mem_mut!(emu).read_multiple::<CPU, u32, _>(mem_addr, emu, rlist.len(), |value| {
                let reg = unsafe { rlist_iter.next().unwrap_unchecked() };
                *get_reg_mut(regs, reg) = value;
            });
        }

        if WRITE_BACK && (WRITE || (CPU == CpuType::ARM9 && unlikely((rlist.0 & !((1 << (op0 as u8 + 1)) - 1)) != 0 || (rlist.0 == (1 << op0 as u8))))) {
            *regs.get_reg_mut(op0) = if DECREMENT { start_addr } else { addr + (rlist.len() << 2) as u32 }
        }

        if USER && unlikely(rlist.is_reserved(Reg::PC)) {
            unsafe { unreachable_unchecked() }
        }
    }
}

macro_rules! imm_breakout {
    ($asm:expr, $pc:expr, $total_cycles:expr) => {{
        crate::logging::debug_println!("immediate breakout");
        let is_thumb = $pc & 1 == 1;
        let pc = $pc & !1;
        if crate::IS_DEBUG {
            $asm.runtime_data.set_branch_out_pc(pc & !1);
        }
        $asm.runtime_data.accumulated_cycles += $total_cycles - $asm.runtime_data.pre_cycle_count_sum;
        let next_pc_offset = (1 << (!is_thumb as u8)) + 2;
        crate::core::emu::get_regs_mut!($asm.emu, CPU).pc = pc + next_pc_offset;
        crate::core::emu::get_mem_mut!($asm.emu).breakout_imm = false;
        crate::jit::jit_asm_common_funs::exit_guest_context!($asm);
    }};
}
pub(super) use imm_breakout;

pub unsafe extern "C" fn inst_mem_handler<const CPU: CpuType, const WRITE: bool, const AMOUNT: MemoryAmount, const SIGNED: bool, const IS_SWP: bool>(
    addr: u32,
    op0: u32,
    pc: u32,
    total_cycles_reg: u32,
) {
    let asm = get_jit_asm_ptr::<CPU>();
    if WRITE {
        handle_request_write::<CPU, AMOUNT>(op0, addr, (*asm).emu, Reg::from((total_cycles_reg >> 16) as u8));
    } else {
        handle_request_read::<CPU, AMOUNT, SIGNED>((op0 as *mut u32).as_mut_unchecked(), addr, (*asm).emu);
    }
    if IS_SWP && unlikely(get_mem!((*asm).emu).breakout_imm) {
        todo!()
    }
    if WRITE && unlikely(get_mem!((*asm).emu).breakout_imm) {
        imm_breakout!((*asm), pc, total_cycles_reg as u16);
    }
}

pub unsafe extern "C" fn inst_mem_handler_multiple<const CPU: CpuType, const WRITE: bool, const USER: bool, const PRE: bool, const WRITE_BACK: bool, const DECREMENT: bool>(
    op0_rlist: u32,
    pc: u32,
    total_cycles: u16,
) {
    let asm = get_jit_asm_ptr::<CPU>();
    handle_multiple_request::<CPU, WRITE, USER, PRE, WRITE_BACK, DECREMENT>(pc, (op0_rlist & 0xFFFF) as u16, (op0_rlist >> 16) as u8, (*asm).emu);
    if WRITE && unlikely(get_mem!((*asm).emu).breakout_imm) {
        imm_breakout!((*asm), pc, total_cycles);
    }
}

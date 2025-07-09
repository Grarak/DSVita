use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::get_jit_asm_ptr;
use crate::jit::assembler::block_asm::GuestInstMetadata;
use crate::jit::assembler::reg_alloc::GUEST_REG_ALLOCATIONS;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::MemoryAmount;
use crate::logging::debug_println;
use bilge::prelude::*;
use handler::*;
use std::arch::naked_asm;
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::intrinsics::{likely, unlikely};
use std::ptr;

mod handler {
    use crate::core::emu::Emu;
    use crate::core::CpuType;
    use crate::jit::assembler::block_asm::GuestInstMetadata;
    use crate::jit::reg::{Reg, RegReserve};
    use crate::jit::MemoryAmount;
    use crate::logging::debug_println;
    use std::hint::assert_unchecked;
    use std::intrinsics::{likely, unlikely};
    use std::mem::MaybeUninit;
    use std::slice;

    pub fn handle_request_write<const CPU: CpuType, const AMOUNT: MemoryAmount>(value0: u32, value1: u32, addr: u32, emu: &mut Emu) {
        match AMOUNT {
            MemoryAmount::Byte => emu.mem_write::<CPU, _>(addr, value0 as u8),
            MemoryAmount::Half => emu.mem_write::<CPU, _>(addr, value0 as u16),
            MemoryAmount::Word => emu.mem_write::<CPU, _>(addr, value0),
            MemoryAmount::Double => {
                emu.mem_write::<CPU, _>(addr, value0);
                emu.mem_write::<CPU, _>(addr + 4, value1);
            }
        }
    }

    pub fn handle_request_read<const CPU: CpuType, const AMOUNT: MemoryAmount, const SIGNED: bool>(op0: Reg, addr: u32, emu: &mut Emu) {
        match AMOUNT {
            MemoryAmount::Byte => {
                let value = if SIGNED {
                    emu.mem_read_with_options::<CPU, true, u8>(addr) as i8 as i32 as u32
                } else {
                    emu.mem_read_with_options::<CPU, true, u8>(addr) as u32
                };
                *emu.thread_get_reg_mut(CPU, op0) = value;
            }
            MemoryAmount::Half => {
                let value = if SIGNED {
                    emu.mem_read_with_options::<CPU, true, u16>(addr) as i16 as i32 as u32
                } else {
                    emu.mem_read_with_options::<CPU, true, u16>(addr) as u32
                };
                *emu.thread_get_reg_mut(CPU, op0) = value;
            }
            MemoryAmount::Word => {
                let value = emu.mem_read_with_options::<CPU, true, u32>(addr);
                let shift = (addr & 0x3) << 3;
                *emu.thread_get_reg_mut(CPU, op0) = value.rotate_right(shift);
            }
            MemoryAmount::Double => {
                let value = emu.mem_read_with_options::<CPU, true, u32>(addr);
                *emu.thread_get_reg_mut(CPU, op0) = value;
                let value = emu.mem_read_with_options::<CPU, true, u32>(addr + 4);
                *emu.thread_get_reg_mut(CPU, Reg::from(op0 as u8 + 1)) = value;
            }
        }
    }

    fn get_reg_usr_mut<const FIQ_MODE: bool>(emu: &mut Emu, cpu: CpuType, reg: Reg) -> &mut u32 {
        if FIQ_MODE || reg == Reg::SP || reg == Reg::LR {
            emu.thread_get_reg_usr_mut(cpu, reg)
        } else {
            emu.thread_get_reg_mut(cpu, reg)
        }
    }

    #[inline(always)]
    pub fn handle_multiple_request<
        const CPU: CpuType,
        const WRITE: bool,
        const WRITE_BACK: bool,
        const DECREMENT: bool,
        const VALID: bool,
        const USER: bool,
        const NEEDS_PC: bool,
        const GX_FIFO: bool,
    >(
        rlist: RegReserve,
        rlist_len: usize,
        op0_reg: Reg,
        pre: bool,
        emu: &mut Emu,
        metadata: &GuestInstMetadata,
    ) {
        let is_thumb = metadata.pc & 1 == 1;
        let pc = metadata.pc & !1;

        let op0 = emu.thread_get_reg_mut(CPU, op0_reg);
        debug_println!("{CPU:?} handle multiple request at {pc:x} addr {op0:x} thumb: {is_thumb} write: {WRITE}");
        debug_assert_ne!(rlist_len, 0);

        let start_addr = if DECREMENT { *op0 - ((rlist_len as u32) << 2) } else { *op0 };
        let addr = start_addr;

        if WRITE_BACK && (!WRITE || VALID || (CPU == CpuType::ARM7 && unlikely((rlist.0 & ((1 << (op0_reg as u8 + 1)) - 1)) > (1 << op0_reg as u8)))) {
            if DECREMENT {
                *op0 = addr;
            } else {
                *op0 = addr + ((rlist_len as u32) << 2);
            }
        }

        let mem_addr = addr + ((pre as u32) << 2);

        let get_reg_mut = if USER && !emu.thread_is_user_mode(CPU) {
            if unlikely(emu.thread_is_fiq_mode(CPU)) {
                get_reg_usr_mut::<true>
            } else {
                get_reg_usr_mut::<false>
            }
        } else {
            Emu::thread_get_reg_mut
        };

        let mut values: [u32; Reg::CPSR as usize] = unsafe { MaybeUninit::uninit().assume_init() };
        unsafe { assert_unchecked(rlist_len <= values.len()) };
        if WRITE {
            let mut rlist = rlist.0.reverse_bits();
            for i in 0..if NEEDS_PC { rlist_len - 1 } else { rlist_len } {
                let zeros = rlist.leading_zeros();
                let reg = Reg::from(zeros as u8);
                rlist &= !(0x80000000 >> zeros);
                unsafe { *values.get_unchecked_mut(i) = *get_reg_mut(emu, CPU, reg) };
            }
            if NEEDS_PC {
                let pc_offset = 4 << (!is_thumb as u8);
                unsafe { *values.get_unchecked_mut(rlist_len - 1) = pc + pc_offset };
            }

            if GX_FIFO && likely(mem_addr >= 0x4000400 && mem_addr < 0x4000440) {
                let end_addr = mem_addr + ((rlist_len as u32) << 2);
                if unlikely(end_addr > 0x4000440) {
                    let diff = (end_addr - 0x4000440) >> 2;
                    let slice = unsafe { slice::from_raw_parts(values.as_ptr(), rlist_len - diff as usize) };
                    emu.regs_3d_set_gx_fifo_multiple(slice);
                    let slice = unsafe { slice::from_raw_parts(values.as_ptr().add(rlist_len - diff as usize), diff as usize) };
                    emu.mem_write_multiple_slice::<CPU, true, _>(mem_addr, slice);
                } else {
                    let slice = unsafe { slice::from_raw_parts(values.as_ptr(), rlist_len) };
                    emu.regs_3d_set_gx_fifo_multiple(slice);
                }
            } else {
                let slice = unsafe { slice::from_raw_parts(values.as_ptr(), rlist_len) };
                emu.mem_write_multiple_slice::<CPU, true, _>(mem_addr, slice);
            }
        } else {
            let slice = unsafe { slice::from_raw_parts_mut(values.as_mut_ptr(), rlist_len) };
            emu.mem_read_multiple_slice::<CPU, true, _>(mem_addr, slice);
            let mut rlist = rlist.0.reverse_bits();
            for i in 0..rlist_len {
                let zeros = rlist.leading_zeros();
                let reg = Reg::from(zeros as u8);
                rlist &= !(0x80000000 >> zeros);
                unsafe { *get_reg_mut(emu, CPU, reg) = *values.get_unchecked(i) };
            }
        }

        if WRITE_BACK && (WRITE || VALID || (CPU == CpuType::ARM9 && unlikely((rlist.0 & !((1 << (op0_reg as u8 + 1)) - 1)) != 0 || (rlist.0 == (1 << op0_reg as u8))))) {
            *emu.thread_get_reg_mut(CPU, op0_reg) = if DECREMENT { start_addr } else { addr + (rlist_len << 2) as u32 };
        }
    }
}

macro_rules! imm_breakout {
    ($cpu:expr, $asm:expr, $pc:expr, $total_cycles:expr) => {{
        crate::logging::debug_println!("immediate breakout");
        let is_thumb = $pc & 1 == 1;
        let pc = $pc & !1;
        if crate::IS_DEBUG {
            $asm.runtime_data.set_branch_out_pc(pc & !1);
        }
        $asm.runtime_data.accumulated_cycles += $total_cycles - $asm.runtime_data.pre_cycle_count_sum;
        let next_pc_offset = (1 << (!is_thumb as u8)) + 2;
        $asm.emu.thread[$cpu].pc = pc + next_pc_offset;
        $asm.emu.breakout_imm = false;
        crate::jit::jit_asm_common_funs::exit_guest_context!($asm);
    }};
}
pub(super) use imm_breakout;

unsafe extern "C" fn breakout_after_write<const CPU: CpuType>(metadata: *const GuestInstMetadata, host_regs: &[usize; GUEST_REG_ALLOCATIONS.len()]) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    debug_println!("{CPU:?} breakout after write");
    for dirty_guest_reg in (*metadata).dirty_guest_regs - Reg::CPSR {
        let mapped_reg = *(*metadata).mapped_guest_regs.get_unchecked(dirty_guest_reg as usize);
        let value = *host_regs.get_unchecked(mapped_reg as usize - 4) as u32;
        debug_println!("{CPU:?} save {dirty_guest_reg:?} as {value:x} from host {mapped_reg:?}");
        *asm.emu.thread_get_reg_mut(CPU, dirty_guest_reg) = value;
    }
    imm_breakout!(CPU, asm, (*metadata).pc, (*metadata).total_cycle_count);
}

unsafe extern "C" fn _inst_write_mem_handler<const CPU: CpuType, const AMOUNT: MemoryAmount>(value0: u32, value1: u32, addr: u32, metadata: *const GuestInstMetadata) -> *const GuestInstMetadata {
    debug_println!("{CPU:?} handle write request at {:x} addr {addr:x}", (*metadata).pc);

    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    handle_request_write::<CPU, AMOUNT>(value0, value1, addr, asm.emu);
    if unlikely(asm.emu.breakout_imm) {
        metadata
    } else {
        ptr::null()
    }
}

macro_rules! write_mem_handler_cpsr {
    ($name:ident, $inst_fun:ident) => {
        #[unsafe(naked)]
        pub unsafe extern "C" fn $name<const CPU: CpuType, const AMOUNT: MemoryAmount>(_value0: u32, _value1: u32, _addr: u32, _metadata: *const GuestInstMetadata) {
            #[rustfmt::skip]
            naked_asm!(
                "push {{r3, lr}}",
                "mrs lr, cpsr",
                "lsrs lr, lr, 24",
                "strb lr, [r3, {cpsr_bits}]",
                "mov r3, r12",
                "bl {handler}",
                "cmp r0, 0",
                "bne 1f",
                "pop {{r3, lr}}",
                "ldr r2, [r3, {cpsr}]",
                "msr cpsr, r2",
                "bx lr",
                "1:",
                "push {{r4-r11}}",
                "mov r1, sp",
                "b {breakout}",
                cpsr_bits = const Reg::CPSR as usize * 4 + 3,
                handler = sym $inst_fun::<CPU, AMOUNT>,
                cpsr = const Reg::CPSR as usize * 4,
                breakout = sym breakout_after_write::<CPU>,
            );
        }
    };
}

macro_rules! write_mem_handler {
    ($name:ident, $inst_fun:ident) => {
        #[unsafe(naked)]
        pub unsafe extern "C" fn $name<const CPU: CpuType, const AMOUNT: MemoryAmount>(_value0: u32, _value1: u32, _addr: u32, _metadata: *const GuestInstMetadata) {
            #[rustfmt::skip]
            naked_asm!(
                "push {{r3, lr}}",
                "mov r3, r12",
                "bl {}",
                "cmp r0, 0",
                "it eq",
                "popeq {{r3, pc}}",
                "push {{r4-r11}}",
                "mov r1, sp",
                "b {}",
                sym $inst_fun::<CPU, AMOUNT>,
                sym breakout_after_write::<CPU>,
            );
        }
    };
}

write_mem_handler_cpsr!(inst_write_mem_handler_with_cpsr, _inst_write_mem_handler);
write_mem_handler!(inst_write_mem_handler, _inst_write_mem_handler);

pub unsafe extern "C" fn _inst_read_mem_handler<const CPU: CpuType, const AMOUNT: MemoryAmount, const SIGNED: bool>(op0: u8, _: u32, addr: u32) -> u32 {
    if AMOUNT == MemoryAmount::Double || (AMOUNT == MemoryAmount::Word && SIGNED) {
        unreachable_unchecked();
    }

    debug_println!("{CPU:?} handle read request addr {addr:x}");

    let asm = get_jit_asm_ptr::<CPU>();
    handle_request_read::<CPU, AMOUNT, SIGNED>(Reg::from(op0), addr, (*asm).emu);
    *(*asm).emu.thread_get_reg(CPU, Reg::from(op0))
}

pub unsafe extern "C" fn _inst_read64_mem_handler<const CPU: CpuType>(op0: u8, _: u32, addr: u32) -> u64 {
    debug_println!("{CPU:?} handle read64 request addr {addr:x}");

    let asm = get_jit_asm_ptr::<CPU>();
    handle_request_read::<CPU, { MemoryAmount::Double }, false>(Reg::from(op0), addr, (*asm).emu);
    let value0 = *(*asm).emu.thread_get_reg(CPU, Reg::from(op0));
    let value1 = *(*asm).emu.thread_get_reg(CPU, Reg::from(op0 + 1));
    (value0 as u64) | ((value1 as u64) << 32)
}

#[unsafe(naked)]
pub unsafe extern "C" fn inst_read_mem_handler<const CPU: CpuType, const AMOUNT: MemoryAmount, const SIGNED: bool>(_: u8, _: u32, _: u32) {
    #[rustfmt::skip]
    naked_asm!(
        "push {{r3, lr}}",
        "bl {}",
        "pop {{r3, pc}}",
        sym _inst_read_mem_handler::<CPU, AMOUNT, SIGNED>,
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn inst_read64_mem_handler<const CPU: CpuType>(_: u8, _: u32, _: u32) {
    #[rustfmt::skip]
    naked_asm!(
        "push {{r3, lr}}",
        "bl {}",
        "pop {{r3, pc}}",
        sym _inst_read64_mem_handler::<CPU>,
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn inst_read_mem_handler_with_cpsr<const CPU: CpuType, const AMOUNT: MemoryAmount, const SIGNED: bool>(_: u8, _: u32, _: u32) {
    #[rustfmt::skip]
    naked_asm!(
        "push {{r3, lr}}",
        "mrs lr, cpsr",
        "lsrs lr, lr, 24",
        "strb lr, [r3, {cpsr_bits}]",
        "bl {handler}",
        "pop {{r3, lr}}",
        "ldr r2, [r3, {cpsr}]",
        "msr cpsr, r2",
        "bx lr",
        cpsr_bits = const Reg::CPSR as usize * 4 + 3,
        handler = sym _inst_read_mem_handler::<CPU, AMOUNT, SIGNED>,
        cpsr = const Reg::CPSR as usize * 4,
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn inst_read64_mem_handler_with_cpsr<const CPU: CpuType>(_: u8, _: u32, _: u32) {
    #[rustfmt::skip]
    naked_asm!(
        "push {{r3, lr}}",
        "mrs lr, cpsr",
        "lsrs lr, lr, 24",
        "strb lr, [r3, {cpsr_bits}]",
        "bl {handler}",
        "pop {{r3, lr}}",
        "ldr r2, [r3, {cpsr}]",
        "msr cpsr, r2",
        "bx lr",
        cpsr_bits = const Reg::CPSR as usize * 4 + 3,
        handler = sym _inst_read64_mem_handler::<CPU>,
        cpsr = const Reg::CPSR as usize * 4,
    );
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct InstMemMultipleParams {
    pub rlist: u16,
    pub rlist_len: u4,
    pub op0: u4,
    pub pre: bool,
    pub user: bool,
    unused: u6,
}

pub unsafe extern "C" fn _inst_mem_handler_multiple<
    const CPU: CpuType,
    const WRITE: bool,
    const WRITE_BACK: bool,
    const DECREMENT: bool,
    const VALID: bool,
    const USER: bool,
    const NEEDS_PC: bool,
    const GX_FIFO: bool,
>(
    params: u32,
    metadata: *const GuestInstMetadata,
    host_regs: &mut [usize; GUEST_REG_ALLOCATIONS.len()],
) {
    if (!WRITE_BACK && !VALID) || (!WRITE && NEEDS_PC) || (!WRITE && GX_FIFO) || (USER && NEEDS_PC) {
        unreachable_unchecked()
    }

    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    let metadata = metadata.as_ref_unchecked();
    let params = InstMemMultipleParams::from(params);
    let op0_reg = Reg::from(u8::from(params.op0()));
    let rlist = RegReserve::from(params.rlist() as u32);
    let rlist_len = u8::from(params.rlist_len()) as usize;

    if WRITE {
        for dirty_guest_reg in metadata.dirty_guest_regs - Reg::CPSR {
            let mapped_reg = *metadata.mapped_guest_regs.get_unchecked(dirty_guest_reg as usize);
            let value = *host_regs.get_unchecked(mapped_reg as usize - 4) as u32;
            *asm.emu.thread_get_reg_mut(CPU, dirty_guest_reg) = value;
        }
    } else {
        let op0 = Reg::from(u8::from(params.op0()));
        if metadata.dirty_guest_regs.is_reserved(op0) {
            let mapped_reg = *metadata.mapped_guest_regs.get_unchecked(op0 as usize);
            let value = *host_regs.get_unchecked(mapped_reg as usize - 4) as u32;
            *asm.emu.thread_get_reg_mut(CPU, op0) = value;
        }
    }

    handle_multiple_request::<CPU, WRITE, WRITE_BACK, DECREMENT, VALID, USER, NEEDS_PC, GX_FIFO>(rlist, rlist_len, op0_reg, params.pre(), asm.emu, metadata);

    if WRITE && unlikely(asm.emu.breakout_imm) {
        imm_breakout!(CPU, asm, metadata.pc, metadata.total_cycle_count);
    } else {
        if WRITE_BACK {
            let mapped_reg = *metadata.mapped_guest_regs.get_unchecked(op0_reg as usize);
            *host_regs.get_unchecked_mut(mapped_reg as usize - 4) = *asm.emu.thread_get_reg(CPU, op0_reg) as usize;
        }

        if !WRITE {
            let mut rlist = rlist.0.reverse_bits();
            for i in 0..rlist_len {
                let zeros = rlist.leading_zeros();
                let reg = Reg::from(zeros as u8);
                rlist &= !(0x80000000 >> zeros);
                let mapped_reg = *metadata.mapped_guest_regs.get_unchecked(reg as usize);
                if mapped_reg != Reg::None {
                    *host_regs.get_unchecked_mut(mapped_reg as usize - 4) = *asm.emu.thread_get_reg(CPU, reg) as usize;
                }
            }
        }
    }
}

macro_rules! write_mem_handler_multiple_cpsr {
    ($name:ident, $inst_func:ident, $gx_fifo:expr) => {
        #[unsafe(naked)]
        pub unsafe extern "C" fn $name<const CPU: CpuType, const WRITE_BACK: bool, const DECREMENT: bool, const VALID: bool, const USER: bool, const NEEDS_PC: bool>(
            _: u32,
            _: *const GuestInstMetadata,
        ) {
            #[rustfmt::skip]
            naked_asm!(
                "push {{r3-r11,lr}}",
                "mrs r2, cpsr",
                "lsrs r2, r2, 24",
                "strb r2, [r3, {cpsr_bits}]",
                "add r2, sp, 4",
                "bl {handler}",
                "pop {{r3-r11,lr}}",
                "ldr r2, [r3, {cpsr}]",
                "msr cpsr, r2",
                "bx lr",
                cpsr_bits = const Reg::CPSR as usize * 4 + 3,
                handler = sym $inst_func::<CPU, true, WRITE_BACK, DECREMENT, VALID, USER, NEEDS_PC, $gx_fifo>,
                cpsr = const Reg::CPSR as usize * 4,
            );
        }
    };
}

macro_rules! write_mem_handler_multiple {
    ($name:ident, $inst_func:ident, $gx_fifo:expr) => {
        #[unsafe(naked)]
        pub unsafe extern "C" fn $name<const CPU: CpuType, const WRITE_BACK: bool, const DECREMENT: bool, const VALID: bool, const USER: bool, const NEEDS_PC: bool>(
            _: u32,
            _: *const GuestInstMetadata,
        ) {
            #[rustfmt::skip]
            naked_asm!(
                "push {{r3-r11,lr}}",
                "add r2, sp, 4",
                "bl {handler}",
                "pop {{r3-r11,pc}}",
                handler = sym $inst_func::<CPU, true, WRITE_BACK, DECREMENT, VALID, USER, NEEDS_PC, $gx_fifo>,
            );
        }
    };
}

write_mem_handler_multiple_cpsr!(inst_write_mem_handler_multiple_with_cpsr, _inst_mem_handler_multiple, false);
write_mem_handler_multiple!(inst_write_mem_handler_multiple, _inst_mem_handler_multiple, false);

#[unsafe(naked)]
pub unsafe extern "C" fn inst_read_mem_handler_multiple_with_cpsr<const CPU: CpuType, const WRITE_BACK: bool, const DECREMENT: bool, const VALID: bool, const USER: bool, const NEEDS_PC: bool>(
    _: u32,
    _: *const GuestInstMetadata,
) {
    #[rustfmt::skip]
    naked_asm!(
        "push {{r3-r11,lr}}",
        "mrs r2, cpsr",
        "lsrs r2, r2, 24",
        "strb r2, [r3, {cpsr_bits}]",
        "add r2, sp, 4",
        "bl {handler}",
        "pop {{r3-r11,lr}}",
        "ldr r2, [r3, {cpsr}]",
        "msr cpsr, r2",
        "bx lr",
        cpsr_bits = const Reg::CPSR as usize * 4 + 3,
        handler = sym _inst_mem_handler_multiple::<CPU, false, WRITE_BACK, DECREMENT, VALID, USER, NEEDS_PC, false>,
        cpsr = const Reg::CPSR as usize * 4,
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn inst_read_mem_handler_multiple<const CPU: CpuType, const WRITE_BACK: bool, const DECREMENT: bool, const VALID: bool, const USER: bool, const NEEDS_PC: bool>(
    _: u32,
    _: *const GuestInstMetadata,
) {
    #[rustfmt::skip]
    naked_asm!(
        "push {{r3-r11,lr}}",
        "add r2, sp, 4",
        "bl {handler}",
        "pop {{r3-r11,pc}}",
        handler = sym _inst_mem_handler_multiple::<CPU, false, WRITE_BACK, DECREMENT, VALID, USER, NEEDS_PC, false>,
    );
}

unsafe extern "C" fn _inst_mem_handler_write_gx_fifo<const CPU: CpuType, const AMOUNT: MemoryAmount>(
    value0: u32,
    value1: u32,
    addr: u32,
    metadata: *const GuestInstMetadata,
) -> *const GuestInstMetadata {
    unsafe { assert_unchecked(CPU == ARM9 && AMOUNT == MemoryAmount::Word) };

    if likely(addr >= 0x4000400 && addr < 0x4000440) {
        let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
        asm.emu.regs_3d_set_gx_fifo(0xFFFFFFFF, value0);
        if unlikely(asm.emu.breakout_imm) {
            metadata
        } else {
            ptr::null()
        }
    } else {
        _inst_write_mem_handler::<{ ARM9 }, { MemoryAmount::Word }>(value0, value1, addr, metadata)
    }
}

write_mem_handler_cpsr!(inst_write_mem_handler_gxfifo_with_cpsr, _inst_mem_handler_write_gx_fifo);
write_mem_handler!(inst_write_mem_handler_gxfifo, _inst_mem_handler_write_gx_fifo);
write_mem_handler_multiple_cpsr!(inst_write_mem_handler_multiple_gxfifo_with_cpsr, _inst_mem_handler_multiple, true);
write_mem_handler_multiple!(inst_write_mem_handler_multiple_gxfifo, _inst_mem_handler_multiple, true);

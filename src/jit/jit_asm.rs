use crate::core::emu::Emu;
use crate::core::hle::bios;
use crate::core::memory::regions;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::analyzer::asm_analyzer::AsmAnalyzer;
use crate::jit::assembler::block_asm::{BlockAsm, GuestInstOffset};
use crate::jit::assembler::vixl::vixl::{FlagsUpdate_DontCare, FlagsUpdate_LeaveFlags};
use crate::jit::assembler::vixl::{Label, MasmAdd5, MasmBl2, MasmBlx1, MasmLdr2, MasmLsr5, MasmMov4, MasmPop1, MasmPush1, MasmSubs3};
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::emitter::map_fun_cpu;
use crate::jit::inst_branch_handler::call_jit_fun;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm_common_funs::exit_guest_context;
use crate::jit::jit_memory::JitMemory;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::reg::{reg_reserve, RegReserve};
use crate::jit::Cond;
use crate::logging::{branch_println, debug_println};
use crate::mmap::Mmap;
use crate::mmap::PAGE_SHIFT;
use crate::{get_jit_asm_ptr, BRANCH_LOG, CURRENT_RUNNING_CPU, DEBUG_LOG, IS_DEBUG, KEEP_FRAME_POINTER};
use bilge::prelude::*;
use static_assertions::const_assert_eq;
use std::arch::{asm, naked_asm};
use std::intrinsics::unlikely;
use std::{mem, slice};
use xxhash_rust::xxh32::xxh32;

#[derive(Default)]
#[cfg(debug_assertions)]
pub struct JitDebugInfo {
    pub inst_offsets: Vec<usize>,
    pub block_offsets: Vec<usize>,
}

#[cfg(debug_assertions)]
impl JitDebugInfo {
    pub fn resize(&mut self, basic_blocks_size: usize, insts_size: usize) {
        self.inst_offsets.resize(insts_size + 1, 0);
        self.block_offsets.resize(basic_blocks_size, 0);
    }

    pub fn record_basic_block_offset(&mut self, basic_block_index: usize, offset: usize) {
        self.block_offsets[basic_block_index] = offset;
    }

    pub fn record_inst_offset(&mut self, inst_index: usize, offset: usize) {
        self.inst_offsets[inst_index] = offset;
    }

    fn print_info(&self, start_pc: u32, thumb: bool) {
        println!("basic block offsets:");
        for (i, offset) in self.block_offsets.iter().enumerate() {
            print!("({i}, 0x{offset:x}),");
        }
        println!();
        println!("insts offsets:");
        for (i, offset) in self.inst_offsets.iter().enumerate() {
            print!("(0x{:x}, 0x{offset:x}),", start_pc + (i << if thumb { 1 } else { 2 }) as u32);
        }
        println!();
    }
}

#[derive(Default)]
#[cfg(not(debug_assertions))]
pub struct JitDebugInfo {}

#[cfg(not(debug_assertions))]
impl JitDebugInfo {
    pub fn resize(&mut self, basic_blocks_size: usize, insts_size: usize) {}
    pub fn record_basic_block_offset(&mut self, basic_block_index: usize, offset: usize) {}
    pub fn record_inst_offset(&mut self, inst_index: usize, offset: usize) {}
    fn print_info(&self, start_pc: u32, thumb: bool) {}
}

pub struct JitBuf {
    pub guest_pc_start: u32,
    pub insts: Vec<InstInfo>,
    pub insts_cycle_counts: Vec<u16>,
    pub debug_info: JitDebugInfo,
}

impl JitBuf {
    fn new() -> Self {
        JitBuf {
            guest_pc_start: 0,
            insts: Vec::new(),
            insts_cycle_counts: Vec::new(),
            debug_info: JitDebugInfo::default(),
        }
    }

    fn clear_all(&mut self) {
        self.insts.clear();
        self.insts_cycle_counts.clear();
    }
}

pub const RETURN_STACK_SIZE: usize = 64;
pub const MAX_STACK_DEPTH_SIZE: usize = 9 * 1024 * 1024;

#[bitsize(32)]
#[derive(FromBits)]
struct JitRuntimeDataPacked {
    return_stack_ptr: u30,
    in_interrupt: bool,
    idle_loop: bool,
}

#[repr(C, align(32))]
pub struct JitRuntimeData {
    pub pre_cycle_count_sum: u16,
    pub accumulated_cycles: u16,
    pub host_sp: usize,
    data_packed: JitRuntimeDataPacked,
    pub return_stack: [u32; RETURN_STACK_SIZE],
    pub interrupt_sp: usize,
    #[cfg(debug_assertions)]
    branch_out_pc: u32,
}

impl JitRuntimeData {
    fn new() -> Self {
        JitRuntimeData {
            pre_cycle_count_sum: 0,
            accumulated_cycles: 0,
            host_sp: 0,
            data_packed: JitRuntimeDataPacked::from(0),
            return_stack: [u32::MAX; RETURN_STACK_SIZE],
            interrupt_sp: 0,
            #[cfg(debug_assertions)]
            branch_out_pc: u32::MAX,
        }
    }

    #[cfg(debug_assertions)]
    pub const fn get_branch_out_pc_offset() -> usize {
        mem::offset_of!(JitRuntimeData, branch_out_pc)
    }

    #[cfg(not(debug_assertions))]
    pub const fn get_branch_out_pc_offset() -> u8 {
        panic!()
    }

    #[cfg(debug_assertions)]
    pub fn set_branch_out_pc(&mut self, pc: u32) {
        self.branch_out_pc = pc;
    }

    #[cfg(not(debug_assertions))]
    pub fn set_branch_out_pc(&mut self, _: u32) {
        panic!()
    }

    #[cfg(debug_assertions)]
    pub fn get_branch_out_pc(&self) -> u32 {
        self.branch_out_pc
    }

    #[cfg(not(debug_assertions))]
    pub fn get_branch_out_pc(&self) -> u32 {
        panic!()
    }

    pub const fn get_pre_cycle_count_sum_offset() -> usize {
        mem::offset_of!(JitRuntimeData, pre_cycle_count_sum)
    }

    pub const fn get_accumulated_cycles_offset() -> usize {
        mem::offset_of!(JitRuntimeData, accumulated_cycles)
    }

    pub const fn get_host_sp_offset() -> usize {
        mem::offset_of!(JitRuntimeData, host_sp)
    }

    pub const fn get_data_packed_offset() -> usize {
        mem::offset_of!(JitRuntimeData, data_packed)
    }

    pub const fn get_return_stack_offset() -> usize {
        mem::offset_of!(JitRuntimeData, return_stack)
    }

    pub fn is_idle_loop(&self) -> bool {
        self.data_packed.idle_loop()
    }

    pub fn set_idle_loop(&mut self, idle_loop: bool) {
        self.data_packed.set_idle_loop(idle_loop);
    }

    pub fn is_in_interrupt(&self) -> bool {
        self.data_packed.in_interrupt()
    }

    pub fn set_in_interrupt(&mut self, in_interrupt: bool) {
        self.data_packed.set_in_interrupt(in_interrupt);
    }

    pub fn get_return_stack_ptr(&self) -> usize {
        u32::from(self.data_packed.return_stack_ptr()) as usize
    }

    pub fn push_return_stack(&mut self, value: u32) {
        let mut return_stack_ptr = self.get_return_stack_ptr();
        unsafe { *self.return_stack.get_unchecked_mut(return_stack_ptr) = value };
        return_stack_ptr += 1;
        return_stack_ptr &= RETURN_STACK_SIZE - 1;
        unsafe { *self.return_stack.get_unchecked_mut(return_stack_ptr) = u32::MAX };
        self.data_packed.set_return_stack_ptr(u30::new(return_stack_ptr as u32));
    }

    pub fn pop_return_stack(&mut self) -> u32 {
        let mut return_stack_ptr = self.get_return_stack_ptr();
        return_stack_ptr = return_stack_ptr.wrapping_sub(1);
        return_stack_ptr &= RETURN_STACK_SIZE - 1;
        self.data_packed.set_return_stack_ptr(u30::new(return_stack_ptr as u32));
        unsafe { *self.return_stack.get_unchecked(return_stack_ptr) }
    }

    pub fn get_sp_depth_size(&self) -> usize {
        let mut sp: usize;
        unsafe { asm!("mov {}, sp", out(reg) sp, options(pure, nomem, preserves_flags)) };
        self.host_sp - sp
    }

    pub fn clear_return_stack_ptr(&mut self) {
        self.data_packed.set_return_stack_ptr(u30::new(0));
        self.return_stack[RETURN_STACK_SIZE - 1] = u32::MAX;
    }
}

pub fn align_guest_pc(guest_pc: u32) -> u32 {
    let thumb = guest_pc & 1 == 1;
    let guest_pc_mask = !(1 | ((!thumb as u32) << 1));
    guest_pc & guest_pc_mask
}

pub extern "C" fn hle_bios_uninterrupt<const CPU: CpuType>() {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut_unchecked() };
    let current_pc = CPU.thread_regs().pc;
    asm.runtime_data.accumulated_cycles += 3;
    bios::uninterrupt::<CPU>(asm.emu);
    if unlikely(asm.emu.cpu_is_halted(CPU)) {
        if IS_DEBUG {
            asm.runtime_data.set_branch_out_pc(current_pc);
        }
        unsafe { exit_guest_context!(asm) };
    } else {
        match CPU {
            ARM9 => {
                if unlikely(asm.runtime_data.is_in_interrupt() && asm.runtime_data.pop_return_stack() == CPU.thread_regs().pc) {
                    asm.emu.thread_set_thumb(CPU, CPU.thread_regs().pc & 1 == 1);
                    unsafe {
                        std::arch::asm!(
                        "mov sp, {}",
                        "pop {{r4-r12,pc}}",
                        in(reg) asm.runtime_data.interrupt_sp
                        );
                        std::hint::unreachable_unchecked();
                    }
                } else {
                    debug_println!("{CPU:?} uninterrupt return lr doesn't match pc");
                    asm.runtime_data.clear_return_stack_ptr();
                    unsafe { call_jit_fun::<CPU>(asm, CPU.thread_regs().pc) };
                }
            }
            ARM7 => {
                asm.runtime_data.clear_return_stack_ptr();
                unsafe { call_jit_fun::<CPU>(asm, CPU.thread_regs().pc) };
            }
        }
    }
}

extern "C" fn guest_block_invalid(guest_pc: u32) {
    debug_println!("Guest block hash mismatch {guest_pc:x}");
    emit_code_block(guest_pc);
}

#[unsafe(naked)]
unsafe extern "C" fn validate_guest_block_hash() {
    #[rustfmt::skip]
    naked_asm!(
        "mov r6, lr",
        "mov r2, 0",
        "bl {xxh32}",
        "cmp r0, r5",
        "mov r0, r4",
        "itt eq",
        "moveq lr, r6",
        "bxeq lr",
        "add sp, sp, 4",
        "pop {{r4-r11,lr}}",
        "b {guest_block_invalid}",
        xxh32 = sym xxh32,
        guest_block_invalid = sym guest_block_invalid,
    );
}

const_assert_eq!(size_of::<Vec<GuestInstOffset>>(), 12);
const_assert_eq!(size_of::<GuestInstOffset>(), 40);

#[unsafe(naked)]
unsafe extern "C" fn jump_to_other_guest_pc<const CPU: CpuType>(_: u32, _: u32) {
    #[rustfmt::skip]
    naked_asm!(
        "mov r1, {jit_asm_ptr}",
        "lsrs r0, r0, 1", // r0 = diff >> 1
        "subs r0, r0, 1", // r0 = r0 - 1
        "ldr r2, [r1, {emu_offset}]", // r2 = asm.emu
        "mov r4, {jit_mem_mmap_offset}",
        "ldr r3, [r2, r4]", // r3 = r2.jit.mem.ptr
        "add r0, r0, r0, lsl #2",
        "sub r3, lr, r3", // r3 = lr - r3
        "mov r5, {jit_guest_inst_offset}",
        "lsrs r3, {page_shift}", // r3 = r3 >> PAGE_SHIFT
        "ldr r4, [r2, r5]", // r4 = &r2.jit.guest_inst_offsets
        "add r3, r3, r3, lsl #1",
        "add r4, r4, r3, lsl #2",
        "ldr r5, [r4, 4]", // r5 = r4[r3], offset by 4, first 4 bytes of vec is capacity
        "add r5, r5, r0, lsl #3",
        "ldmia r5, {{r2, r4, r5, r6, r7, r8, r9, r10, r11}}",
        "uxth r0, r2",
        "lsrs r2, r2, 16",
        "strh r2, [r1, {pre_cycle_count_sum_offset}]",
        "ldr r4, [r4]",
        "ldr r5, [r5]",
        "ldr r6, [r6]",
        "ldr r7, [r7]",
        "ldr r8, [r8]",
        "mov r3, {guest_regs_offset}",
        "ldr r9, [r9]",
        "ldr r2, [r3, {cpsr_offset}]",
        "ldr r10, [r10]",
        "msr cpsr, r2",
        "ldr r11, [r11]",
        "bx lr",
        jit_asm_ptr = const CPU.jit_asm_addr(),
        emu_offset = const mem::offset_of!(JitAsm, emu),
        jit_mem_mmap_offset = const mem::offset_of!(Emu, jit) + mem::offset_of!(JitMemory, mem) + mem::offset_of!(Mmap, ptr),
        page_shift = const PAGE_SHIFT,
        jit_guest_inst_offset = const mem::offset_of!(Emu, jit) + mem::offset_of!(JitMemory, guest_inst_offsets),
        pre_cycle_count_sum_offset = const mem::offset_of!(JitAsm, runtime_data) + mem::offset_of!(JitRuntimeData, pre_cycle_count_sum),
        guest_regs_offset = const CPU.guest_regs_addr(),
        cpsr_offset = const Reg::CPSR as usize * 4,
    );
}

#[cold]
pub extern "C" fn emit_code_block(guest_pc: u32) {
    let thumb = (guest_pc & 1) == 1;
    let cpu = unsafe { CURRENT_RUNNING_CPU };
    let asm = match cpu {
        ARM9 => unsafe { get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked() },
        ARM7 => unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() },
    };
    emit_code_block_internal(cpu, asm, guest_pc & !1, thumb);
}

fn emit_code_block_internal(cpu: CpuType, asm: &mut JitAsm, guest_pc: u32, thumb: bool) {
    let mut uncond_branch_count = 0;
    let mut pc_offset = 0;
    let get_inst_info = if thumb {
        |cpu: CpuType, asm: &mut JitAsm, pc| {
            let opcode = match cpu {
                ARM9 => asm.emu.mem_read::<{ ARM9 }, u16>(pc),
                ARM7 => asm.emu.mem_read::<{ ARM7 }, u16>(pc),
            };
            let (op, func) = lookup_thumb_opcode(opcode);
            InstInfo::from(func(opcode, *op))
        }
    } else {
        |cpu: CpuType, asm: &mut JitAsm, pc| {
            let opcode = match cpu {
                ARM9 => asm.emu.mem_read::<{ ARM9 }, u32>(pc),
                ARM7 => asm.emu.mem_read::<{ ARM7 }, u32>(pc),
            };
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        }
    };

    let pc_step = if thumb { 2 } else { 4 };
    let mut heavy_inst_count = 0;
    let mut last_inst_branch = false;

    loop {
        let inst_info = get_inst_info(cpu, asm, guest_pc + pc_offset);

        if inst_info.op == Op::UnkArm || inst_info.op == Op::UnkThumb || inst_info.cond == Cond::NV {
            break;
        }

        if let Some(last) = asm.jit_buf.insts_cycle_counts.last() {
            debug_assert!(u16::MAX - last >= inst_info.cycle as u16, "{cpu:?} {guest_pc:x} {inst_info:?}");
            asm.jit_buf.insts_cycle_counts.push(last + inst_info.cycle as u16);
        } else {
            asm.jit_buf.insts_cycle_counts.push(inst_info.cycle as u16);
            debug_assert!(asm.jit_buf.insts_cycle_counts.len() <= u16::MAX as usize, "{cpu:?} {guest_pc:x} {inst_info:?}")
        }

        let is_uncond_branch = inst_info.is_uncond_branch();
        if is_uncond_branch {
            uncond_branch_count += 1;
        }
        let is_unreturnable_branch = !inst_info.out_regs.is_reserved(Reg::LR) && is_uncond_branch;
        let op = inst_info.op;
        if op.is_single_mem_transfer() || op.is_multiple_mem_transfer() || op.is_branch() {
            heavy_inst_count += 1;
        }
        asm.jit_buf.insts.push(inst_info);

        if is_unreturnable_branch || uncond_branch_count == 20 {
            last_inst_branch = true;
            break;
        }

        if heavy_inst_count > 50 && op != Op::BlSetupT {
            break;
        }
        pc_offset += pc_step;
    }

    let (jit_entry, flushed) = {
        debug_println!("{cpu:?} {thumb} emit code block {guest_pc:x} - {:x}", guest_pc + pc_offset);
        // unsafe { BLOCK_LOG = guest_pc == 0x200675e };

        asm.analyzer.analyze(guest_pc, &asm.jit_buf.insts, thumb);
        asm.jit_buf.guest_pc_start = guest_pc;
        asm.jit_buf.debug_info.resize(asm.analyzer.basic_blocks.len(), asm.jit_buf.insts.len());

        let mut block_asm = BlockAsm::new(cpu, thumb);
        block_asm.prologue(asm.analyzer.basic_blocks.len());

        if cpu == ARM7 && guest_pc & 0xFF000000 != regions::VRAM_OFFSET && asm.emu.settings.arm7_block_validation() {
            let guest_ptr = ARM7.mmu_tcm_addr() + (guest_pc as usize & 0xFFFFFFF);
            let size = (pc_offset + pc_step) as usize;
            let hash = xxh32(unsafe { slice::from_raw_parts(guest_ptr as _, size) }, 0);

            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R4, &Reg::R0.into());
            block_asm.ldr2(Reg::R0, guest_ptr as u32);
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &(size as u32).into());
            block_asm.ldr2(Reg::R5, hash);
            block_asm.call(validate_guest_block_hash as _);
        }

        if BRANCH_LOG {
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R4, &Reg::R0.into());
            block_asm.call(map_fun_cpu!(cpu, debug_enter_block));
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &Reg::R4.into());
        }

        let mut default_pc_label = Label::new();

        let pc = guest_pc | (thumb as u32);
        block_asm.ldr2(Reg::R1, pc);
        block_asm.subs3(Reg::R0, Reg::R0, &Reg::R1.into());
        block_asm.bl2(Cond::EQ, &mut default_pc_label);
        if !thumb {
            block_asm.lsr5(FlagsUpdate_DontCare, Cond::AL, Reg::R0, Reg::R0, &1.into());
        }
        if KEEP_FRAME_POINTER {
            block_asm.push1(reg_reserve!(Reg::R7, Reg::R11));
        }
        block_asm.ldr2(Reg::R3, map_fun_cpu!(cpu, jump_to_other_guest_pc) as u32);
        block_asm.blx1(Reg::R3);
        if KEEP_FRAME_POINTER {
            block_asm.pop1(reg_reserve!(Reg::R7, Reg::R11));
        }
        block_asm.add5(FlagsUpdate_LeaveFlags, Cond::AL, Reg::PC, Reg::PC, &Reg::R0.into());

        block_asm.bind(&mut default_pc_label);
        block_asm.set_guest_start();
        asm.emit(&mut block_asm, thumb);

        if !last_inst_branch {
            let next_pc = guest_pc + pc_offset + if thumb { 3 } else { 4 };
            block_asm.ldr2(Reg::R0, next_pc);
            block_asm.store_guest_reg(Reg::R0, Reg::PC);
            asm.emit_branch_external_label(asm.jit_buf.insts.len() - 1, asm.analyzer.basic_blocks.len() - 1, next_pc, false, &mut block_asm);
        }

        block_asm.finalize();

        // let opcodes = block_asm.get_code_buffer();
        // if IS_DEBUG && guest_pc == 0x2020618 {
        //     asm.jit_buf.debug_info.print_info(guest_pc, thumb);
        //     for &opcode in opcodes {
        //         print!("0x{opcode:x},");
        //     }
        //     println!();
        //     todo!()
        // }
        let (insert_entry, flushed) = asm.emu.jit_insert_block(block_asm, guest_pc, guest_pc + pc_offset + pc_step, thumb, cpu);
        let jit_entry: extern "C" fn(u32) = unsafe { mem::transmute(insert_entry) };

        if DEBUG_LOG {
            // println!("{CPU:?} Mapping {guest_pc:#010x} to {:#010x}", jit_entry as *const fn() as usize);
        }
        asm.jit_buf.clear_all();
        (jit_entry, flushed)
    };

    jit_entry(guest_pc | (thumb as u32));
    if flushed {
        unsafe { exit_guest_context!(asm) };
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn call_jit_entry(_: u32, _entry: *const fn(), _host_sp: *mut usize) {
    #[rustfmt::skip]
    naked_asm!(
        "push {{r4-r12,lr}}",
        "str sp, [r2]",
        "blx r1",
        "pop {{r4-r12,pc}}",
    );
}

fn execute_internal<const CPU: CpuType>(guest_pc: u32) -> u16 {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut_unchecked() };

    let thumb = (guest_pc & 1) == 1;
    let guest_pc = align_guest_pc(guest_pc);
    debug_println!("{:?} Execute {:x} thumb {}", CPU, guest_pc | (thumb as u32), thumb);

    let jit_entry = {
        asm.emu.thread_set_thumb(CPU, thumb);

        let jit_entry = asm.emu.jit.get_jit_start_addr(guest_pc);

        debug_println!("{CPU:?} Enter jit addr {:x}", jit_entry as usize);

        if IS_DEBUG {
            asm.runtime_data.set_branch_out_pc(u32::MAX);
        }
        asm.runtime_data.pre_cycle_count_sum = 0;
        asm.runtime_data.accumulated_cycles = 0;
        asm.runtime_data.clear_return_stack_ptr();
        asm.runtime_data.data_packed = JitRuntimeDataPacked::from(0);
        asm.emu.breakout_imm = false;
        jit_entry
    };

    unsafe { call_jit_entry(guest_pc | (thumb as u32), jit_entry as _, &mut asm.runtime_data.host_sp) };

    if IS_DEBUG {
        assert_ne!(
            asm.runtime_data.get_branch_out_pc(),
            u32::MAX,
            "{CPU:?} idle loop {} return stack ptr {}",
            asm.runtime_data.is_idle_loop(),
            asm.runtime_data.get_return_stack_ptr(),
        );
    }

    if BRANCH_LOG {
        branch_println!(
            "{CPU:?} reading opcode of breakout at {:x} executed cycles {}",
            asm.runtime_data.get_branch_out_pc(),
            asm.runtime_data.accumulated_cycles,
        );
        if asm.runtime_data.is_idle_loop() {
            branch_println!("{CPU:?} idle loop");
        }
        let inst_info = if asm.emu.thread_is_thumb(CPU) {
            let opcode = asm.emu.mem_read::<CPU, _>(asm.runtime_data.get_branch_out_pc());
            let (op, func) = lookup_thumb_opcode(opcode);
            InstInfo::from(func(opcode, *op))
        } else {
            let opcode = asm.emu.mem_read::<CPU, _>(asm.runtime_data.get_branch_out_pc());
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        };
        debug_inst_info::<CPU>(asm.emu, asm.runtime_data.get_branch_out_pc(), &format!("breakout\n\t{CPU:?} {inst_info:?}"));
    }

    asm.runtime_data.accumulated_cycles
}

pub struct JitAsm<'a> {
    pub cpu: CpuType,
    pub emu: &'a mut Emu,
    pub jit_buf: JitBuf,
    pub runtime_data: JitRuntimeData,
    pub analyzer: AsmAnalyzer,
}

impl<'a> JitAsm<'a> {
    #[inline(never)]
    pub fn new(cpu: CpuType, emu: &'a mut Emu) -> Self {
        JitAsm {
            cpu,
            emu,
            jit_buf: JitBuf::new(),
            runtime_data: JitRuntimeData::new(),
            analyzer: AsmAnalyzer::default(),
        }
    }

    pub fn execute<const CPU: CpuType>(&mut self) -> u16 {
        let entry = CPU.thread_regs().pc;
        execute_internal::<CPU>(entry)
    }
}

fn debug_inst_info<const CPU: CpuType>(emu: &Emu, pc: u32, append: &str) {
    let mut output = "Executed ".to_owned();

    for reg in reg_reserve!(Reg::SP, Reg::LR, Reg::PC, Reg::CPSR, Reg::SPSR) + RegReserve::gp() {
        let value = if reg != Reg::PC { *emu.thread_get_reg(CPU, reg) } else { pc };
        output += &format!("{reg:?}: {value:x}, ");
    }

    println!("{CPU:?} {output}{append}");
}

pub unsafe extern "C" fn debug_after_exec_op<const CPU: CpuType>(pc: u32, opcode: u32) {
    let asm = get_jit_asm_ptr::<CPU>();
    let inst_info = {
        if (*asm).emu.thread_is_thumb(CPU) {
            let (op, func) = lookup_thumb_opcode(opcode as u16);
            InstInfo::from(func(opcode as u16, *op))
        } else {
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        }
    };

    debug_inst_info::<CPU>((*asm).emu, pc, &format!("\n\t{CPU:?} {inst_info:?}"));
}

unsafe extern "C" fn debug_enter_block<const CPU: CpuType>(pc: u32) {
    branch_println!("{CPU:?} execute {pc:x}");
    let asm = get_jit_asm_ptr::<CPU>();
    debug_inst_info::<CPU>((*asm).emu, pc, "enter block");
}

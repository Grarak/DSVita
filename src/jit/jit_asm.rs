use crate::core::emu::Emu;
use crate::core::hle::bios;
use crate::core::memory::regions;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::analyzer::asm_analyzer::AsmAnalyzer;
use crate::jit::assembler::block_asm::{BlockAsm, GuestInstOffset};
use crate::jit::assembler::reg_alloc::GUEST_REGS_LENGTH;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::emitter::map_fun_cpu;
use crate::jit::inst_branch_handler::call_jit_fun;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm_common_funs::exit_guest_context;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::reg::{reg_reserve, RegReserve};
use crate::jit::Cond;
use crate::logging::{branch_println, debug_println};
use crate::mmap::PAGE_SHIFT;
use crate::{get_jit_asm_ptr, BRANCH_LOG, CURRENT_RUNNING_CPU, DEBUG_LOG, IS_DEBUG};
use bilge::prelude::*;
use static_assertions::const_assert_eq;
use std::arch::{asm, naked_asm};
use std::intrinsics::unlikely;
use std::{mem, slice};
use vixl::{BranchHint_kNear, FlagsUpdate_DontCare, FlagsUpdate_LeaveFlags, Label, MasmAdd5, MasmB3, MasmBlx1, MasmLdr2, MasmLsr5, MasmMov4, MasmSubs3};
use xxhash_rust::xxh32::xxh32;

pub static mut BLOCK_LOG: bool = false;

#[derive(Default)]
#[cfg(any(debug_assertions, target_os = "linux"))]
pub struct JitDebugInfo {
    pub inst_offsets: Vec<usize>,
    pub block_offsets: Vec<usize>,
    pub blocks: Vec<(u32, usize, usize)>,
}

#[cfg(any(debug_assertions, target_os = "linux"))]
impl JitDebugInfo {
    pub fn resize(&mut self, basic_blocks_size: usize, insts_size: usize) {
        self.inst_offsets.resize(insts_size + 1, 0);
        self.block_offsets.resize(basic_blocks_size, 0);
        self.blocks.clear();
    }

    pub fn record_basic_block_offset(&mut self, basic_block_index: usize, offset: usize) {
        self.block_offsets[basic_block_index] = offset;
    }

    pub fn record_basic_block(&mut self, basic_block_start_pc: u32, offset: usize, size: usize) {
        self.blocks.push((basic_block_start_pc, offset, size));
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
#[cfg(all(not(debug_assertions), not(target_os = "linux")))]
pub struct JitDebugInfo {}

#[cfg(all(not(debug_assertions), not(target_os = "linux")))]
impl JitDebugInfo {
    pub fn resize(&mut self, basic_blocks_size: usize, insts_size: usize) {}
    pub fn record_basic_block_offset(&mut self, basic_block_index: usize, offset: usize) {}
    pub fn record_basic_block(&mut self, basic_block_start_pc: u32, offset: usize, size: usize) {}
    pub fn record_inst_offset(&mut self, inst_index: usize, offset: usize) {}
    fn print_info(&self, start_pc: u32, thumb: bool) {}
}

pub struct JitForwardBranch {
    pub inst_index: usize,
    pub target_pc: u32,
    pub dirty_guest_regs: RegReserve,
    pub guest_regs_mapping: [Reg; GUEST_REGS_LENGTH],
    pub bind_label: Label,
}

impl JitForwardBranch {
    pub fn new(inst_index: usize, target_pc: u32, dirty_guest_regs: RegReserve, guest_regs_mapping: [Reg; GUEST_REGS_LENGTH], bind_label: Label) -> Self {
        JitForwardBranch {
            inst_index,
            target_pc,
            dirty_guest_regs,
            guest_regs_mapping,
            bind_label,
        }
    }
}

pub struct JitRunSchedulerLabel {
    pub inst_index: usize,
    pub target_pc: u32,
    pub dirty_guest_regs: RegReserve,
    pub guest_regs_mapping: [Reg; GUEST_REGS_LENGTH],
    pub bind_label: Label,
    pub continue_label: Label,
    pub exit_label: Option<Label>,
}

impl JitRunSchedulerLabel {
    pub fn new(
        inst_index: usize,
        target_pc: u32,
        dirty_guest_regs: RegReserve,
        guest_regs_mapping: [Reg; GUEST_REGS_LENGTH],
        bind_label: Label,
        continue_label: Label,
        exit_label: Option<Label>,
    ) -> Self {
        JitRunSchedulerLabel {
            inst_index,
            target_pc,
            dirty_guest_regs,
            guest_regs_mapping,
            bind_label,
            continue_label,
            exit_label,
        }
    }
}

pub struct JitCondIndirectBranch {
    pub inst_index: usize,
    pub dirty_guest_regs: RegReserve,
    pub guest_regs_mapping: [Reg; GUEST_REGS_LENGTH],
    pub bind_label: Label,
}

impl JitCondIndirectBranch {
    pub fn new(inst_index: usize, dirty_guest_regs: RegReserve, guest_regs_mapping: [Reg; GUEST_REGS_LENGTH], bind_label: Label) -> Self {
        JitCondIndirectBranch {
            inst_index,
            dirty_guest_regs,
            guest_regs_mapping,
            bind_label,
        }
    }
}

pub struct JitBuf {
    pub guest_pc_start: u32,
    pub insts: Vec<InstInfo>,
    pub insts_cycle_counts: Vec<u16>,
    pub forward_branches: Vec<JitForwardBranch>,
    pub run_scheduler_labels: Vec<JitRunSchedulerLabel>,
    pub cond_indirect_branches: Vec<JitCondIndirectBranch>,
    pub debug_info: JitDebugInfo,
}

impl JitBuf {
    fn new() -> Self {
        JitBuf {
            guest_pc_start: 0,
            insts: Vec::new(),
            insts_cycle_counts: Vec::new(),
            forward_branches: Vec::new(),
            run_scheduler_labels: Vec::new(),
            cond_indirect_branches: Vec::new(),
            debug_info: JitDebugInfo::default(),
        }
    }

    fn clear_all(&mut self) {
        self.insts.clear();
        self.insts_cycle_counts.clear();
        self.forward_branches.clear();
        self.run_scheduler_labels.clear();
        self.cond_indirect_branches.clear();
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
        "mov r3, {guest_regs_offset}",
        "ldr r5, [r4, 4]", // r5 = r4[r3], offset by 4, first 4 bytes of vec is capacity
        "add r5, r5, r0, lsl #3",
        "ldmia r5, {{r2, r4, r5, r6, r7, r8, r9, r10, r11}}",
        "ldr r0, [r3, {cpsr_offset}]",
        "msr cpsr, r0",
        "ldr r4, [r4]",
        "uxth r0, r2",
        "lsr r2, r2, 16",
        "strh r2, [r1, {pre_cycle_count_sum_offset}]",
        "ldr r5, [r5]",
        "ldr r6, [r6]",
        "ldr r7, [r7]",
        "ldr r8, [r8]",
        "ldr r9, [r9]",
        "ldr r10, [r10]",
        "ldr r11, [r11]",
        "bx lr",
        jit_asm_ptr = const CPU.jit_asm_addr(),
        emu_offset = const mem::offset_of!(JitAsm, emu),
        jit_mem_mmap_offset = const mem::offset_of!(Emu, jit.mem.ptr),
        page_shift = const PAGE_SHIFT,
        jit_guest_inst_offset = const mem::offset_of!(Emu, jit.guest_inst_offsets),
        pre_cycle_count_sum_offset = const mem::offset_of!(JitAsm, runtime_data.pre_cycle_count_sum),
        guest_regs_offset = const CPU.guest_regs_addr(),
        cpsr_offset = const Reg::CPSR as usize * 4,
    );
}

#[cold]
pub extern "C" fn emit_code_block(guest_pc: u32) {
    let thumb = (guest_pc & 1) == 1;
    let asm = match unsafe { CURRENT_RUNNING_CPU } {
        ARM9 => unsafe { get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked() },
        ARM7 => unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() },
    };
    emit_code_block_internal(asm, guest_pc & !1, thumb);
}

fn emit_code_block_internal(asm: &mut JitAsm, guest_pc: u32, thumb: bool) {
    let pc_step = if thumb { 2 } else { 4 };

    let is_os_irq_handler = if asm.emu.nitro_sdk_version.is_valid() {
        if asm.cpu == ARM7 && asm.os_irq_handler_addr & 0xFF000000 != regions::SHARED_WRAM_OFFSET {
            asm.os_irq_handler_addr = asm.emu.mem_read::<{ ARM7 }, u32>(0x380FFFC);
        }
        asm.os_irq_handler_addr != 0 && guest_pc == asm.os_irq_handler_addr
    } else {
        false
    };

    asm.jit_buf.clear_all();
    let guest_pc_end = JitAsm::fill_jit_insts_buf(asm.cpu, &mut asm.jit_buf.insts, &mut asm.jit_buf.insts_cycle_counts, asm.emu, guest_pc, thumb, is_os_irq_handler);

    if asm.cpu == ARM9 && is_os_irq_handler && asm.emit_hle_os_irq_handler(guest_pc, thumb) {
        return;
    }

    if asm.emit_nitrosdk_func(guest_pc, thumb) {
        return;
    }

    let (jit_entry, flushed) = {
        debug_println!("{:?} {thumb} emit code block {guest_pc:x} - {guest_pc_end:x}", asm.cpu);
        // unsafe { BLOCK_LOG = guest_pc == 0x206a3a4 };

        asm.analyzer.analyze(guest_pc, &asm.jit_buf.insts, thumb);
        asm.jit_buf.guest_pc_start = guest_pc;
        asm.jit_buf.debug_info.resize(asm.analyzer.basic_blocks.len(), asm.jit_buf.insts.len());

        let mut block_asm = BlockAsm::new(asm.cpu, thumb, is_os_irq_handler);
        block_asm.prologue(asm.analyzer.basic_blocks.len());

        if asm.cpu == ARM7 && guest_pc & 0xFF000000 != regions::VRAM_OFFSET && !asm.emu.nitro_sdk_version.is_valid() {
            let guest_ptr = ARM7.mmu_tcm_addr() + (guest_pc as usize & 0xFFFFFFF);
            let size = (guest_pc_end - guest_pc + pc_step) as usize;
            let hash = xxh32(unsafe { slice::from_raw_parts(guest_ptr as _, size) }, 0);

            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R4, &Reg::R0.into());
            block_asm.ldr2(Reg::R0, guest_ptr as u32);
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &(size as u32).into());
            block_asm.ldr2(Reg::R5, hash);
            block_asm.call(validate_guest_block_hash as _);
        }

        if BRANCH_LOG {
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R4, &Reg::R0.into());
            block_asm.call(map_fun_cpu!(asm.cpu, debug_enter_block));
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &Reg::R4.into());
        }

        let mut default_pc_label = Label::new();

        let pc = guest_pc | (thumb as u32);
        block_asm.ldr2(Reg::R1, pc);
        block_asm.subs3(Reg::R0, Reg::R0, &Reg::R1.into());
        block_asm.b3(Cond::EQ, &mut default_pc_label, BranchHint_kNear);
        if !thumb {
            block_asm.lsr5(FlagsUpdate_DontCare, Cond::AL, Reg::R0, Reg::R0, &1.into());
        }
        block_asm.ldr2(Reg::R3, map_fun_cpu!(asm.cpu, jump_to_other_guest_pc) as u32);
        block_asm.blx1(Reg::R3);
        block_asm.add5(FlagsUpdate_LeaveFlags, Cond::AL, Reg::PC, Reg::PC, &Reg::R0.into());

        block_asm.bind(&mut default_pc_label);
        block_asm.set_guest_start();

        asm.emit_fs_clear_overlay_image_hook(guest_pc, thumb, &mut block_asm);

        asm.emit(&mut block_asm, thumb);

        let last_inst = asm.jit_buf.insts.last().unwrap();
        if !last_inst.is_uncond_branch() {
            let next_pc = guest_pc_end + if thumb { 3 } else { 4 };
            block_asm.ldr2(Reg::R0, next_pc);
            block_asm.store_guest_reg(Reg::R0, Reg::PC);
            asm.emit_branch_external_label(asm.jit_buf.insts.len() - 1, asm.analyzer.basic_blocks.len() - 1, next_pc, false, &mut block_asm);
        }

        asm.emit_epilogue(&mut block_asm);
        block_asm.finalize();

        let opcodes = block_asm.get_code_buffer();
        // if IS_DEBUG && unsafe { BLOCK_LOG } {
        //     asm.jit_buf.debug_info.print_info(guest_pc, thumb);
        //     for &opcode in opcodes {
        //         print!("0x{opcode:x},");
        //     }
        //     println!();
        //     todo!()
        // }
        let (insert_entry, flushed) = asm.emu.jit_insert_block(block_asm, &asm.jit_buf.debug_info, guest_pc, guest_pc_end + pc_step, thumb, asm.cpu);
        let jit_entry: extern "C" fn(u32) = unsafe { mem::transmute(insert_entry) };

        if DEBUG_LOG {
            // println!("{CPU:?} Mapping {guest_pc:#010x} to {:#010x}", jit_entry as *const fn() as usize);
        }
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
    pub os_irq_handler_addr: u32,
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
            os_irq_handler_addr: 0,
        }
    }

    pub fn execute<const CPU: CpuType>(&mut self) -> u16 {
        let entry = CPU.thread_regs().pc;
        execute_internal::<CPU>(entry)
    }

    pub fn fill_jit_insts_buf(cpu: CpuType, insts: &mut Vec<InstInfo>, cycle_counts: &mut Vec<u16>, emu: &mut Emu, guest_pc: u32, thumb: bool, until_bx: bool) -> u32 {
        let mut pc_offset = 0;
        let get_inst_info = if thumb {
            |cpu, emu: &mut Emu, pc| {
                let opcode = match cpu {
                    ARM9 => emu.mem_read::<{ ARM9 }, u16>(pc),
                    ARM7 => emu.mem_read::<{ ARM7 }, u16>(pc),
                };
                let (op, func) = lookup_thumb_opcode(opcode);
                InstInfo::from(func(opcode, *op))
            }
        } else {
            |cpu, emu: &mut Emu, pc| {
                let opcode = match cpu {
                    ARM9 => emu.mem_read::<{ ARM9 }, u32>(pc),
                    ARM7 => emu.mem_read::<{ ARM7 }, u32>(pc),
                };
                let (op, func) = lookup_opcode(opcode);
                func(opcode, *op)
            }
        };

        let pc_step = if thumb { 2 } else { 4 };

        let mut min_imm_guest_addr = u32::MAX;
        while guest_pc + pc_offset < min_imm_guest_addr {
            let pc = guest_pc + pc_offset;
            let inst_info = get_inst_info(cpu, emu, pc);

            if inst_info.op == Op::UnkArm || inst_info.op == Op::UnkThumb || inst_info.cond == Cond::NV {
                break;
            }

            if let Some(last) = cycle_counts.last() {
                debug_assert!(u16::MAX - last >= inst_info.cycle as u16, "{cpu:?} {guest_pc:x} {inst_info:?}");
                cycle_counts.push(last + inst_info.cycle as u16);
            } else {
                cycle_counts.push(inst_info.cycle as u16);
                debug_assert!(cycle_counts.len() <= u16::MAX as usize, "{cpu:?} {guest_pc:x} {inst_info:?}");
            }

            if let Some(imm_addr) = inst_info.imm_transfer_addr(pc) {
                if imm_addr > pc && imm_addr < min_imm_guest_addr {
                    min_imm_guest_addr = imm_addr;
                }
            }

            let is_uncond_branch = inst_info.is_uncond_branch();
            let cond = inst_info.cond;
            let is_unreturnable_branch = !inst_info.out_regs.is_reserved(Reg::LR) && is_uncond_branch;
            let op = inst_info.op;
            insts.push(inst_info);

            if (matches!(op, Op::Bx | Op::BxRegT) && cond == Cond::AL) || (insts.len() >= 500 && op != Op::BlSetupT) || (!until_bx && is_unreturnable_branch && min_imm_guest_addr == u32::MAX) {
                break;
            }

            pc_offset += pc_step;
        }

        guest_pc + pc_offset
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
    if BRANCH_LOG {
        debug_inst_info::<CPU>((*asm).emu, pc, "enter block");
    }
}

use crate::core::emu::{get_cpu_regs, get_jit, get_jit_mut, get_regs, get_regs_mut, Emu};
use crate::core::hle::bios;
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::block_asm::{BlockAsm, BLOCK_LOG};
use crate::jit::assembler::{BasicBlocksCache, BlockAsmBuf};
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_branch_handler::call_jit_fun;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm_common_funs::{exit_guest_context, JitAsmCommonFuns};
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::reg::{reg_reserve, RegReserve};
use crate::logging::debug_println;
use crate::{get_jit_asm_ptr, CURRENT_RUNNING_CPU, DEBUG_LOG, IS_DEBUG};
use std::arch::{asm, naked_asm};
use std::cell::UnsafeCell;
use std::intrinsics::unlikely;
use std::{mem, ptr};

pub struct JitBuf {
    pub insts: Vec<InstInfo>,
    pub insts_cycle_counts: Vec<u16>,
    pub current_index: usize,
    pub current_pc: u32,
}

impl JitBuf {
    fn new() -> Self {
        JitBuf {
            insts: Vec::new(),
            insts_cycle_counts: Vec::new(),
            current_index: 0,
            current_pc: 0,
        }
    }

    fn clear_all(&mut self) {
        self.insts.clear();
        self.insts_cycle_counts.clear();
    }

    pub fn current_inst(&self) -> &InstInfo {
        unsafe { self.insts.get_unchecked(self.current_index) }
    }

    pub fn current_inst_mut(&mut self) -> &mut InstInfo {
        unsafe { self.insts.get_unchecked_mut(self.current_index) }
    }
}

pub const RETURN_STACK_SIZE: usize = 32;
pub const MAX_STACK_DEPTH_SIZE: usize = 9 * 1024 * 1024;

#[repr(C, align(32))]
pub struct JitRuntimeData {
    pub pre_cycle_count_sum: u16,
    pub accumulated_cycles: u16,
    pub host_sp: usize,
    pub idle_loop_in_interrupt_return_stack_ptr: u8,
    pub return_stack: [u32; RETURN_STACK_SIZE],
    pub interrupt_sp: usize,
    #[cfg(debug_assertions)]
    branch_out_pc: u32,
}

impl JitRuntimeData {
    fn new() -> Self {
        let instance = JitRuntimeData {
            pre_cycle_count_sum: 0,
            accumulated_cycles: 0,
            host_sp: 0,
            idle_loop_in_interrupt_return_stack_ptr: 0,
            return_stack: [0; RETURN_STACK_SIZE],
            interrupt_sp: 0,
            #[cfg(debug_assertions)]
            branch_out_pc: u32::MAX,
        };
        assert_eq!(size_of_val(&instance.return_stack), RETURN_STACK_SIZE * 4);
        instance
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

    pub const fn get_idle_loop_in_interrupt_return_stack_ptr_offset() -> usize {
        mem::offset_of!(JitRuntimeData, idle_loop_in_interrupt_return_stack_ptr)
    }

    pub const fn get_return_stack_offset() -> usize {
        mem::offset_of!(JitRuntimeData, return_stack)
    }

    pub fn is_idle_loop(&self) -> bool {
        self.idle_loop_in_interrupt_return_stack_ptr & 0x80 != 0
    }

    pub fn set_idle_loop(&mut self, idle_loop: bool) {
        self.idle_loop_in_interrupt_return_stack_ptr = (self.idle_loop_in_interrupt_return_stack_ptr & !0x80) | ((idle_loop as u8) << 7)
    }

    pub fn is_in_interrupt(&self) -> bool {
        self.idle_loop_in_interrupt_return_stack_ptr & 0x40 != 0
    }

    pub fn set_in_interrupt(&mut self, in_interrupt: bool) {
        self.idle_loop_in_interrupt_return_stack_ptr = (self.idle_loop_in_interrupt_return_stack_ptr & !0x40) | ((in_interrupt as u8) << 6)
    }

    pub fn get_return_stack_ptr(&self) -> u8 {
        self.idle_loop_in_interrupt_return_stack_ptr & 0x3F
    }

    pub fn push_return_stack(&mut self, value: u32) {
        let mut return_stack_ptr = self.get_return_stack_ptr();
        unsafe { *self.return_stack.get_unchecked_mut(return_stack_ptr as usize) = value };
        return_stack_ptr += 1;
        return_stack_ptr &= RETURN_STACK_SIZE as u8 - 1;
        self.idle_loop_in_interrupt_return_stack_ptr = (self.idle_loop_in_interrupt_return_stack_ptr & 0xC0) | return_stack_ptr;
    }

    pub fn pop_return_stack(&mut self) -> u32 {
        let mut return_stack_ptr = self.get_return_stack_ptr();
        return_stack_ptr = return_stack_ptr.wrapping_sub(1);
        return_stack_ptr &= RETURN_STACK_SIZE as u8 - 1;
        self.idle_loop_in_interrupt_return_stack_ptr = (self.idle_loop_in_interrupt_return_stack_ptr & 0xC0) | return_stack_ptr;
        unsafe { *self.return_stack.get_unchecked(return_stack_ptr as usize) }
    }

    pub fn get_sp_depth_size(&self) -> usize {
        let mut sp: usize;
        unsafe { asm!("mov {}, sp", out(reg) sp, options(pure, nomem, preserves_flags)) };
        self.host_sp - sp
    }

    pub fn clear_return_stack_ptr(&mut self) {
        self.idle_loop_in_interrupt_return_stack_ptr &= 0xC0;
        self.return_stack[RETURN_STACK_SIZE - 1] = 0;
    }
}

pub fn align_guest_pc(guest_pc: u32) -> u32 {
    let thumb = guest_pc & 1 == 1;
    let guest_pc_mask = !(1 | ((!thumb as u32) << 1));
    guest_pc & guest_pc_mask
}

pub extern "C" fn hle_bios_uninterrupt<const CPU: CpuType>() {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut_unchecked() };
    let regs = get_regs_mut!(asm.emu, CPU);
    let current_pc = regs.pc;
    asm.runtime_data.accumulated_cycles += 3;
    bios::uninterrupt::<CPU>(asm.emu);
    if unlikely(get_cpu_regs!(asm.emu, CPU).is_halted()) {
        if IS_DEBUG {
            asm.runtime_data.set_branch_out_pc(current_pc);
        }
        unsafe { exit_guest_context!(asm) };
    } else {
        match CPU {
            ARM9 => {
                if unlikely(asm.runtime_data.is_in_interrupt() && asm.runtime_data.pop_return_stack() == regs.pc) {
                    regs.set_thumb(regs.pc & 1 == 1);
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
                    unsafe { call_jit_fun(asm, regs.pc) };
                }
            }
            ARM7 => {
                asm.runtime_data.clear_return_stack_ptr();
                unsafe { call_jit_fun(asm, regs.pc) };
            }
        }
    }
}

pub extern "C" fn emit_code_block() {
    match unsafe { CURRENT_RUNNING_CPU } {
        ARM9 => {
            let asm = unsafe { get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked() };
            let guest_pc = get_regs!(asm.emu, ARM9).pc;
            let aligned_guest_pc = align_guest_pc(guest_pc);
            let thumb = (guest_pc & 1) == 1;
            emit_code_block_internal::<{ ARM9 }>(asm, aligned_guest_pc, thumb)
        }
        ARM7 => {
            let asm = unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() };
            let guest_pc = get_regs!(asm.emu, ARM7).pc;
            let aligned_guest_pc = align_guest_pc(guest_pc);
            let thumb = (guest_pc & 1) == 1;
            emit_code_block_internal::<{ ARM7 }>(asm, aligned_guest_pc, thumb)
        }
    }
}

fn emit_code_block_internal<const CPU: CpuType>(asm: &mut JitAsm<CPU>, guest_pc: u32, thumb: bool) {
    let mut uncond_branch_count = 0;
    let mut pc_offset = 0;
    let get_inst_info = if thumb {
        |asm: &mut JitAsm<CPU>, pc| {
            let opcode = asm.emu.mem_read::<CPU, u16>(pc);
            let (op, func) = lookup_thumb_opcode(opcode);
            InstInfo::from(func(opcode, *op))
        }
    } else {
        |asm: &mut JitAsm<CPU>, pc| {
            let opcode = asm.emu.mem_read::<CPU, u32>(pc);
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        }
    };
    let pc_step = if thumb { 2 } else { 4 };
    loop {
        let inst_info = get_inst_info(asm, guest_pc + pc_offset);

        if inst_info.op == Op::UnkArm || inst_info.op == Op::UnkThumb {
            break;
        }

        if let Some(last) = asm.jit_buf.insts_cycle_counts.last() {
            debug_assert!(u16::MAX - last >= inst_info.cycle as u16, "{CPU:?} {guest_pc:x} {inst_info:?}");
            asm.jit_buf.insts_cycle_counts.push(last + inst_info.cycle as u16);
        } else {
            asm.jit_buf.insts_cycle_counts.push(inst_info.cycle as u16);
            debug_assert!(asm.jit_buf.insts_cycle_counts.len() <= u16::MAX as usize, "{CPU:?} {guest_pc:x} {inst_info:?}")
        }

        let is_uncond_branch = inst_info.is_uncond_branch();
        if is_uncond_branch {
            uncond_branch_count += 1;
        }
        let is_unreturnable_branch = !inst_info.out_regs.is_reserved(Reg::LR) && is_uncond_branch;
        asm.jit_buf.insts.push(inst_info);

        if is_unreturnable_branch || uncond_branch_count == 4 {
            break;
        }
        pc_offset += pc_step;
    }

    let (jit_entry, flushed) = {
        // println!("{CPU:?} {thumb} emit code block {guest_pc:x}");
        // unsafe { BLOCK_LOG = guest_pc == 0x200675e };

        let guest_regs_ptr = get_regs_mut!(asm.emu, CPU).get_reg_mut_ptr();
        let host_sp_ptr = ptr::addr_of_mut!(asm.runtime_data.host_sp);
        let basic_blocks_cache = asm.basic_blocks_cache.get_mut();
        let block_asm_buf = asm.block_asm_buf.get_mut();
        let mut block_asm = unsafe { BlockAsm::new(false, guest_regs_ptr, host_sp_ptr, mem::transmute(basic_blocks_cache), mem::transmute(block_asm_buf), thumb) };

        if DEBUG_LOG {
            block_asm.call1(debug_enter_block::<CPU> as *const (), guest_pc | (thumb as u32));
            block_asm.restore_reg(Reg::CPSR);
        }

        let emit_func = if thumb { JitAsm::emit_thumb } else { JitAsm::emit };

        let pc_shift = if thumb { 1 } else { 2 };
        for i in 0..asm.jit_buf.insts.len() {
            asm.jit_buf.current_index = i;
            asm.jit_buf.current_pc = guest_pc + (i << pc_shift) as u32;
            debug_println!("{CPU:?} emitting {:?} at pc: {:x}", asm.jit_buf.current_inst(), asm.jit_buf.current_pc);

            // if asm.jit_buf.current_pc == 0x207616c {
            //     block_asm.bkpt(1);
            // }

            emit_func(asm, &mut block_asm);

            if DEBUG_LOG {
                block_asm.save_context();
                block_asm.call2(debug_after_exec_op::<CPU> as *const (), asm.jit_buf.current_pc, asm.jit_buf.current_inst().opcode);
                block_asm.restore_reg(Reg::CPSR);
            }
        }

        block_asm.force_end();

        block_asm.emit_opcodes(guest_pc);
        let opcodes = block_asm.finalize();
        if IS_DEBUG && unsafe { BLOCK_LOG } {
            for &opcode in opcodes {
                println!("0x{opcode:x},");
            }
            todo!()
        }
        let (insert_entry, flushed) = get_jit_mut!(asm.emu).insert_block(opcodes, guest_pc, CPU, asm.emu);
        let jit_entry: extern "C" fn() = unsafe { mem::transmute(insert_entry) };

        if DEBUG_LOG {
            // println!("{CPU:?} Mapping {guest_pc:#010x} to {:#010x}", jit_entry as *const fn() as usize);
        }
        asm.jit_buf.clear_all();
        (jit_entry, flushed)
    };

    jit_entry();
    if flushed {
        unsafe { exit_guest_context!(asm) };
    }
}

#[naked]
unsafe extern "C" fn call_jit_entry(_: *const fn(), _: *mut usize) {
    #[rustfmt::skip]
    naked_asm!(
        "push {{r4-r12,lr}}",
        "str sp, [r1]",
        "blx r0",
        "pop {{r4-r12,pc}}",
    );
}

fn execute_internal<const CPU: CpuType>(guest_pc: u32) -> u16 {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut_unchecked() };

    let thumb = (guest_pc & 1) == 1;
    debug_println!("{:?} Execute {:x} thumb {}", CPU, guest_pc, thumb);

    let jit_entry = {
        get_regs_mut!(asm.emu, CPU).set_thumb(thumb);

        let jit_entry = get_jit!(asm.emu).get_jit_start_addr(align_guest_pc(guest_pc));

        debug_println!("{CPU:?} Enter jit addr {:x}", jit_entry as usize);

        if IS_DEBUG {
            asm.runtime_data.set_branch_out_pc(u32::MAX);
        }
        asm.runtime_data.pre_cycle_count_sum = 0;
        asm.runtime_data.accumulated_cycles = 0;
        asm.runtime_data.clear_return_stack_ptr();
        asm.runtime_data.idle_loop_in_interrupt_return_stack_ptr = 0;
        jit_entry
    };

    unsafe { call_jit_entry(jit_entry as _, &mut asm.runtime_data.host_sp) };

    if IS_DEBUG {
        assert_ne!(
            asm.runtime_data.get_branch_out_pc(),
            u32::MAX,
            "{CPU:?} idle loop {} return stack ptr {}",
            asm.runtime_data.is_idle_loop(),
            asm.runtime_data.get_return_stack_ptr(),
        );
    }

    if DEBUG_LOG {
        println!(
            "{CPU:?} reading opcode of breakout at {:x} executed cycles {}",
            asm.runtime_data.get_branch_out_pc(),
            asm.runtime_data.accumulated_cycles,
        );
        if asm.runtime_data.is_idle_loop() {
            println!("{CPU:?} idle loop");
        }
        let inst_info = if get_regs!(asm.emu, CPU).is_thumb() {
            let opcode = asm.emu.mem_read::<CPU, _>(asm.runtime_data.get_branch_out_pc());
            let (op, func) = lookup_thumb_opcode(opcode);
            InstInfo::from(func(opcode, *op))
        } else {
            let opcode = asm.emu.mem_read::<CPU, _>(asm.runtime_data.get_branch_out_pc());
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        };
        debug_inst_info::<CPU>(get_regs!(asm.emu, CPU), asm.runtime_data.get_branch_out_pc(), &format!("breakout\n\t{:?} {:?}", CPU, inst_info));
    }

    asm.runtime_data.accumulated_cycles
}

pub struct JitAsm<'a, const CPU: CpuType> {
    pub emu: &'a mut Emu,
    pub jit_buf: JitBuf,
    pub runtime_data: JitRuntimeData,
    pub jit_common_funs: JitAsmCommonFuns<CPU>,
    basic_blocks_cache: UnsafeCell<BasicBlocksCache>,
    block_asm_buf: UnsafeCell<BlockAsmBuf>,
}

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    #[inline(never)]
    pub fn new(emu: &'a mut Emu) -> Self {
        JitAsm {
            emu,
            jit_buf: JitBuf::new(),
            runtime_data: JitRuntimeData::new(),
            jit_common_funs: JitAsmCommonFuns::default(),
            basic_blocks_cache: UnsafeCell::new(BasicBlocksCache::new()),
            block_asm_buf: UnsafeCell::new(BlockAsmBuf::new()),
        }
    }

    #[inline(never)]
    pub fn init_common_funs(&mut self) {
        self.jit_common_funs = JitAsmCommonFuns::new(self);
    }

    pub fn execute(&mut self) -> u16 {
        let entry = get_regs!(self.emu, CPU).pc;
        execute_internal::<CPU>(entry)
    }
}

fn debug_inst_info<const CPU: CpuType>(regs: &ThreadRegs, pc: u32, append: &str) {
    let mut output = "Executed ".to_owned();

    for reg in reg_reserve!(Reg::SP, Reg::LR, Reg::PC, Reg::CPSR, Reg::SPSR) + RegReserve::gp() {
        let value = if reg != Reg::PC { *regs.get_reg(reg) } else { pc };
        output += &format!("{:?}: {:x}, ", reg, value);
    }

    println!("{:?} {}{}", CPU, output, append);
}

unsafe extern "C" fn debug_after_exec_op<const CPU: CpuType>(pc: u32, opcode: u32) {
    let asm = get_jit_asm_ptr::<CPU>();
    let inst_info = {
        if get_regs!((*asm).emu, CPU).is_thumb() {
            let (op, func) = lookup_thumb_opcode(opcode as u16);
            InstInfo::from(func(opcode as u16, *op))
        } else {
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        }
    };

    debug_inst_info::<CPU>(get_regs!((*asm).emu, CPU), pc, &format!("\n\t{:?} {:?}", CPU, inst_info));
}

extern "C" fn debug_enter_block<const CPU: CpuType>(pc: u32) {
    println!("{CPU:?} execute {pc:x}");
}

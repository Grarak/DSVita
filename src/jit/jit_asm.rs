use crate::core::emu::{get_cpu_regs, get_jit, get_jit_mut, get_regs, get_regs_mut, Emu};
use crate::core::hle::bios;
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::jit::assembler::block_asm::{BlockAsm, BLOCK_LOG};
use crate::jit::assembler::BlockAsmBuf;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_branch_handler::call_jit_fun;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm_common_funs::{exit_guest_context, JitAsmCommonFuns};
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::reg::{reg_reserve, RegReserve};
use crate::logging::debug_println;
use crate::{get_jit_asm_ptr, DEBUG_LOG, IS_DEBUG};
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
        &self.insts[self.current_index]
    }

    pub fn current_inst_mut(&mut self) -> &mut InstInfo {
        &mut self.insts[self.current_index]
    }
}

pub const RETURN_STACK_SIZE: usize = 8;

#[repr(C, align(32))]
pub struct JitRuntimeData {
    pub pre_cycle_count_sum: u16,
    pub accumulated_cycles: u16,
    pub host_sp: usize,
    pub idle_loop: bool,
    pub return_stack_ptr: u8,
    pub return_stack: [u32; RETURN_STACK_SIZE],
    pub branch_out_pc: u32,
}

impl JitRuntimeData {
    fn new() -> Self {
        let instance = JitRuntimeData {
            branch_out_pc: u32::MAX,
            pre_cycle_count_sum: 0,
            accumulated_cycles: 0,
            idle_loop: false,
            host_sp: 0,
            return_stack_ptr: 0,
            return_stack: [0; RETURN_STACK_SIZE],
        };
        assert_eq!(size_of_val(&instance.return_stack), RETURN_STACK_SIZE * 4);
        instance
    }

    pub const fn get_out_pc_offset() -> u8 {
        mem::offset_of!(JitRuntimeData, branch_out_pc) as u8
    }

    pub const fn get_pre_cycle_count_sum_offset() -> u8 {
        mem::offset_of!(JitRuntimeData, pre_cycle_count_sum) as u8
    }

    pub const fn get_accumulated_cycles_offset() -> u8 {
        mem::offset_of!(JitRuntimeData, accumulated_cycles) as u8
    }

    pub const fn get_idle_loop_offset() -> u8 {
        mem::offset_of!(JitRuntimeData, idle_loop) as u8
    }

    pub const fn get_host_sp_offset() -> u8 {
        mem::offset_of!(JitRuntimeData, host_sp) as u8
    }

    pub const fn get_return_stack_ptr_offset() -> u8 {
        mem::offset_of!(JitRuntimeData, return_stack_ptr) as u8
    }

    pub const fn get_return_stack_offset() -> u8 {
        mem::offset_of!(JitRuntimeData, return_stack) as u8
    }
}

pub fn align_guest_pc(guest_pc: u32) -> u32 {
    let thumb = guest_pc & 1 == 1;
    let guest_pc_mask = !(1 | ((!thumb as u32) << 1));
    guest_pc & guest_pc_mask
}

pub extern "C" fn hle_bios_uninterrupt<const CPU: CpuType>(store_host_sp: bool) {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut().unwrap_unchecked() };
    if IS_DEBUG {
        asm.runtime_data.branch_out_pc = get_regs!(asm.emu, CPU).pc;
    }
    asm.runtime_data.return_stack_ptr = 0;
    asm.runtime_data.accumulated_cycles += 3;
    bios::uninterrupt::<CPU>(asm.emu);
    if unlikely(get_cpu_regs!(asm.emu, CPU).is_halted()) {
        if !store_host_sp {
            unsafe { exit_guest_context!(asm) };
        }
    } else {
        unsafe { call_jit_fun(asm, get_regs_mut!(asm.emu, CPU).pc, store_host_sp) };
    }
}

pub extern "C" fn emit_code_block<const CPU: CpuType>(store_host_sp: bool) {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut().unwrap_unchecked() };

    let guest_pc = get_regs!(asm.emu, CPU).pc;
    let thumb = (guest_pc & 1) == 1;
    if thumb {
        emit_code_block_internal::<CPU, true>(asm, store_host_sp, guest_pc & !1)
    } else {
        emit_code_block_internal::<CPU, false>(asm, store_host_sp, guest_pc & !3)
    }
}

fn emit_code_block_internal<const CPU: CpuType, const THUMB: bool>(asm: &mut JitAsm<CPU>, store_host_sp: bool, guest_pc: u32) {
    let mut uncond_branch_count = 0;
    let mut pc_offset = 0;
    loop {
        let inst_info = if THUMB {
            let opcode = asm.emu.mem_read::<CPU, u16>(guest_pc + pc_offset);
            let (op, func) = lookup_thumb_opcode(opcode);
            InstInfo::from(func(opcode, *op))
        } else {
            let opcode = asm.emu.mem_read::<CPU, u32>(guest_pc + pc_offset);
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        };

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
        pc_offset += if THUMB { 2 } else { 4 };
    }

    let jit_entry = {
        // println!("{CPU:?} {THUMB} emit code block {guest_pc:x}");
        // unsafe { BLOCK_LOG = true };

        let mut block_asm = asm.new_block_asm(false);

        if DEBUG_LOG {
            block_asm.call1(debug_enter_block::<CPU> as *const (), guest_pc | (THUMB as u32));
            block_asm.restore_reg(Reg::CPSR);
        }

        for i in 0..asm.jit_buf.insts.len() {
            asm.jit_buf.current_index = i;
            asm.jit_buf.current_pc = guest_pc + (i << if THUMB { 1 } else { 2 }) as u32;
            debug_println!("{CPU:?} emitting {:?} at pc: {:x}", asm.jit_buf.current_inst(), asm.jit_buf.current_pc);

            // if asm.jit_buf.current_pc == 0x1ff8150 {
            // block_asm.bkpt(1);
            // }

            if THUMB {
                asm.emit_thumb(&mut block_asm);
            } else {
                asm.emit(&mut block_asm);
            }

            if DEBUG_LOG {
                block_asm.save_context();
                block_asm.call2(debug_after_exec_op::<CPU> as *const (), asm.jit_buf.current_pc, asm.jit_buf.current_inst().opcode);
                block_asm.restore_reg(Reg::CPSR);
            }
        }

        block_asm.epilogue();

        let opcodes_len = block_asm.emit_opcodes(guest_pc, THUMB);
        let next_jit_entry = get_jit!(asm.emu).get_next_entry(opcodes_len);
        let opcodes = block_asm.finalize(next_jit_entry);
        if IS_DEBUG && unsafe { BLOCK_LOG } {
            for &opcode in opcodes {
                println!("0x{opcode:x},");
            }
            todo!()
        }
        let (insert_entry, flushed) = get_jit_mut!(asm.emu).insert_block::<CPU>(opcodes, guest_pc, asm.emu);
        if unlikely(flushed) {
            asm.runtime_data.return_stack_ptr = 0;
        }
        let jit_entry: extern "C" fn(bool) = unsafe { mem::transmute(insert_entry) };

        if DEBUG_LOG {
            // println!("{CPU:?} Mapping {guest_pc:#010x} to {:#010x}", jit_entry as *const fn() as usize);
        }
        asm.jit_buf.clear_all();
        jit_entry
    };

    jit_entry(store_host_sp);
}

fn execute_internal<const CPU: CpuType>(guest_pc: u32) -> u16 {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut().unwrap_unchecked() };

    let thumb = (guest_pc & 1) == 1;
    debug_println!("{:?} Execute {:x} thumb {}", CPU, guest_pc, thumb);

    let jit_entry = {
        get_regs_mut!(asm.emu, CPU).set_thumb(thumb);

        let jit_entry = get_jit!(asm.emu).get_jit_start_addr::<CPU>(align_guest_pc(guest_pc));
        let jit_entry: extern "C" fn(bool) = unsafe { mem::transmute(jit_entry) };

        debug_println!("{CPU:?} Enter jit addr {:x}", jit_entry as usize);

        if IS_DEBUG {
            asm.runtime_data.branch_out_pc = u32::MAX;
        }
        asm.runtime_data.pre_cycle_count_sum = 0;
        asm.runtime_data.accumulated_cycles = 0;
        asm.runtime_data.return_stack_ptr = 0;
        jit_entry
    };

    jit_entry(true);

    debug_assert_ne!(asm.runtime_data.branch_out_pc, u32::MAX);

    if DEBUG_LOG {
        println!(
            "{CPU:?} reading opcode of breakout at {:x} executed cycles {}",
            asm.runtime_data.branch_out_pc, asm.runtime_data.accumulated_cycles,
        );
        if asm.runtime_data.idle_loop {
            println!("{CPU:?} idle loop");
        }
        let inst_info = if get_regs!(asm.emu, CPU).is_thumb() {
            let opcode = asm.emu.mem_read::<CPU, _>(asm.runtime_data.branch_out_pc);
            let (op, func) = lookup_thumb_opcode(opcode);
            InstInfo::from(func(opcode, *op))
        } else {
            let opcode = asm.emu.mem_read::<CPU, _>(asm.runtime_data.branch_out_pc);
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        };
        debug_inst_info::<CPU>(get_regs!(asm.emu, CPU), asm.runtime_data.branch_out_pc, &format!("breakout\n\t{:?} {:?}", CPU, inst_info));
    }

    asm.runtime_data.accumulated_cycles
}

pub struct JitAsm<'a, const CPU: CpuType> {
    pub emu: &'a mut Emu,
    pub jit_buf: JitBuf,
    pub runtime_data: JitRuntimeData,
    block_asm_buf: UnsafeCell<BlockAsmBuf>,
    pub jit_common_funs: JitAsmCommonFuns<CPU>,
}

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    #[inline(never)]
    pub fn new(emu: &'a mut Emu) -> Self {
        JitAsm {
            emu,
            jit_buf: JitBuf::new(),
            runtime_data: JitRuntimeData::new(),
            block_asm_buf: UnsafeCell::new(BlockAsmBuf::new()),
            jit_common_funs: JitAsmCommonFuns::default(),
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

    pub fn new_block_asm(&mut self, is_common_fun: bool) -> BlockAsm<'static> {
        let guest_regs_ptr = get_regs_mut!(self.emu, CPU).get_reg_mut_ptr();
        let host_sp_ptr = ptr::addr_of_mut!(self.runtime_data.host_sp);
        unsafe { (*self.block_asm_buf.get()).new_asm(is_common_fun, guest_regs_ptr, host_sp_ptr) }
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

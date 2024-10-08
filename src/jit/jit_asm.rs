use crate::core::emu::{get_cpu_regs, get_jit, get_jit_mut, get_regs, get_regs_mut, Emu};
use crate::core::hle::bios;
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::jit::assembler::block_asm::BLOCK_LOG;
use crate::jit::assembler::BlockAsmBuf;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::reg::{reg_reserve, RegReserve};
use crate::logging::debug_println;
use crate::{get_jit_asm_ptr, DEBUG_LOG, IS_DEBUG};
use std::arch::asm;
use std::cell::UnsafeCell;
use std::hint::unreachable_unchecked;
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

pub const RETURN_STACK_SIZE: usize = 32;

#[repr(C)]
pub struct JitRuntimeData {
    pub branch_out_pc: u32,
    pub pre_cycle_count_sum: u16,
    pub accumulated_cycles: u16,
    pub idle_loop: bool,
    pub host_sp: usize,
    pub return_stack_ptr: u8,
    pub return_stack: [u32; RETURN_STACK_SIZE],
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

    pub fn get_addr(&self) -> *const u32 {
        ptr::addr_of!(self.branch_out_pc)
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
            // r4-r12,pc since we need an even amount of registers for 8 byte alignment, in case the compiler decides to use neon instructions
            unsafe {
                asm!(
                    "mov sp, {}",
                    "pop {{r4-r12,pc}}",
                    in(reg) asm.runtime_data.host_sp
                );
                unreachable_unchecked();
            }
        }
    } else {
        let jit_entry = get_jit!(asm.emu).get_jit_start_addr::<CPU>(get_regs!(asm.emu, CPU).pc);
        let jit_entry: extern "C" fn(bool) = unsafe { mem::transmute(jit_entry) };
        jit_entry(store_host_sp);
    }
}

pub extern "C" fn emit_code_block<const CPU: CpuType>(store_host_sp: bool) {
    let (guest_pc, thumb) = {
        let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut().unwrap_unchecked() };

        let guest_pc = get_regs!(asm.emu, CPU).pc;
        (guest_pc, (guest_pc & 1) == 1)
    };
    if thumb {
        emit_code_block_internal::<CPU, true>(store_host_sp, guest_pc & !1)
    } else {
        emit_code_block_internal::<CPU, false>(store_host_sp, guest_pc & !3)
    }
}

fn emit_code_block_internal<const CPU: CpuType, const THUMB: bool>(store_host_sp: bool, guest_pc: u32) {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut().unwrap_unchecked() };

    {
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

            let is_unreturnable_branch = !inst_info.out_regs.is_reserved(Reg::LR) && inst_info.is_uncond_branch();
            asm.jit_buf.insts.push(inst_info);

            if is_unreturnable_branch {
                break;
            }
            pc_offset += if THUMB { 2 } else { 4 };
        }
    }

    let jit_entry = {
        // unsafe { BLOCK_LOG = guest_pc == 0x20b2688 };

        let guest_regs_ptr = get_regs_mut!(asm.emu, CPU).get_reg_mut_ptr();
        let mut block_asm = unsafe { (*asm.block_asm_buf.get()).new_asm(guest_regs_ptr, ptr::addr_of_mut!((*asm).runtime_data.host_sp)) };

        if DEBUG_LOG {
            block_asm.call1(debug_enter_block::<CPU> as *const (), guest_pc | (THUMB as u32));
            block_asm.restore_reg(Reg::CPSR);
        }

        // if guest_pc == 0x2001b5e {
        //     block_asm.bkpt(2);
        // }

        for i in 0..asm.jit_buf.insts.len() {
            asm.jit_buf.current_index = i;
            asm.jit_buf.current_pc = guest_pc + (i << if THUMB { 1 } else { 2 }) as u32;
            debug_println!("{CPU:?} emitting {:?} at pc: {:x}", asm.jit_buf.current_inst(), asm.jit_buf.current_pc);

            // if asm.jit_buf.current_pc == 0x2001b5c {
            //     block_asm.bkpt(1);
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

        let opcodes = block_asm.finalize(guest_pc, THUMB);
        if unsafe { BLOCK_LOG } {
            for &opcode in &opcodes {
                println!("0x{opcode:x},");
            }
            todo!()
        }
        let (insert_entry, flushed) = get_jit_mut!(asm.emu).insert_block::<CPU>(&opcodes, guest_pc);
        if unlikely(flushed) {
            asm.runtime_data.return_stack_ptr = 0;
        }
        let jit_entry: extern "C" fn(bool) = unsafe { mem::transmute(insert_entry) };

        if DEBUG_LOG {
            println!("{CPU:?} Mapping {guest_pc:#010x} to {:#010x}", jit_entry as *const fn() as usize);
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

        let guest_pc_mask = !(1 | ((!thumb as u32) << 1));
        let guest_pc = guest_pc & guest_pc_mask;
        let jit_entry = get_jit!(asm.emu).get_jit_start_addr::<CPU>(guest_pc);
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
            "{:?} reading opcode of breakout at {:x} executed cycles {}",
            CPU, asm.runtime_data.branch_out_pc, asm.runtime_data.accumulated_cycles
        );
        let inst_info = if thumb {
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
    pub block_asm_buf: UnsafeCell<BlockAsmBuf>,
}

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    #[inline(never)]
    pub fn new(emu: &'a mut Emu) -> Self {
        JitAsm {
            emu,
            jit_buf: JitBuf::new(),
            runtime_data: JitRuntimeData::new(),
            block_asm_buf: UnsafeCell::new(BlockAsmBuf::new()),
        }
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
    println!("{CPU:?} execute {pc:x}")
}

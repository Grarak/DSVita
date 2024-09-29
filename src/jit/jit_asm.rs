use crate::core::emu::{get_jit, get_jit_mut, get_regs, get_regs_mut, Emu};
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
use crate::{get_jit_asm_ptr, DEBUG_LOG, DEBUG_LOG_BRANCH_OUT};
use std::cell::UnsafeCell;
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

#[repr(C)]
pub struct JitRuntimeData {
    pub branch_out_pc: u32,
    pub branch_out_total_cycles: u16,
    pub pre_cycle_count_sum: u16,
    pub accumulated_cycles: u16,
    pub idle_loop: bool,
}

impl JitRuntimeData {
    fn new() -> Self {
        let instance = JitRuntimeData {
            branch_out_pc: u32::MAX,
            branch_out_total_cycles: 0,
            pre_cycle_count_sum: 0,
            accumulated_cycles: 0,
            idle_loop: false,
        };
        let branch_out_pc_ptr = ptr::addr_of!(instance.branch_out_pc) as usize;
        let branch_out_total_cycles_ptr = ptr::addr_of!(instance.branch_out_total_cycles) as usize;
        let pre_cycle_count_sum_ptr = ptr::addr_of!(instance.pre_cycle_count_sum) as usize;
        let accumulated_cycles_ptr = ptr::addr_of!(instance.accumulated_cycles) as usize;
        let idle_loop_ptr = ptr::addr_of!(instance.idle_loop) as usize;
        assert_eq!(branch_out_total_cycles_ptr - branch_out_pc_ptr, Self::get_out_total_cycles_offset() as usize);
        assert_eq!(pre_cycle_count_sum_ptr - branch_out_pc_ptr, Self::get_pre_cycle_count_sum_offset() as usize);
        assert_eq!(accumulated_cycles_ptr - branch_out_pc_ptr, Self::get_accumulated_cycles_offset() as usize);
        assert_eq!(idle_loop_ptr - branch_out_pc_ptr, Self::get_idle_loop_offset() as usize);
        instance
    }

    pub fn get_addr(&self) -> *const u32 {
        ptr::addr_of!(self.branch_out_pc)
    }

    pub const fn get_out_pc_offset() -> u8 {
        0
    }

    pub const fn get_out_total_cycles_offset() -> u8 {
        Self::get_out_pc_offset() + 4
    }

    pub const fn get_pre_cycle_count_sum_offset() -> u8 {
        Self::get_out_total_cycles_offset() + 2
    }

    pub const fn get_accumulated_cycles_offset() -> u8 {
        Self::get_pre_cycle_count_sum_offset() + 2
    }

    pub const fn get_idle_loop_offset() -> u8 {
        Self::get_accumulated_cycles_offset() + 2
    }
}

pub extern "C" fn emit_code_block<const CPU: CpuType>() {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut().unwrap_unchecked() };

    let guest_pc = get_regs!(asm.emu, CPU).pc;
    let thumb = (guest_pc & 1) == 1;
    if thumb {
        emit_code_block_internal::<CPU, true>(guest_pc & !1)
    } else {
        emit_code_block_internal::<CPU, false>(guest_pc & !3)
    }
}

fn emit_code_block_internal<const CPU: CpuType, const THUMB: bool>(guest_pc: u32) {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut().unwrap_unchecked() };

    {
        let mut index = 0;
        loop {
            let inst_info = if THUMB {
                let opcode = asm.emu.mem_read::<CPU, u16>(guest_pc + index);
                let (op, func) = lookup_thumb_opcode(opcode);
                InstInfo::from(func(opcode, *op))
            } else {
                let opcode = asm.emu.mem_read::<CPU, u32>(guest_pc + index);
                let (op, func) = lookup_opcode(opcode);
                func(opcode, *op)
            };

            if let Some(last) = asm.jit_buf.insts_cycle_counts.last() {
                assert!(u16::MAX - last >= inst_info.cycle as u16, "{CPU:?} {guest_pc:x} {inst_info:?}");
                asm.jit_buf.insts_cycle_counts.push(last + inst_info.cycle as u16);
            } else {
                asm.jit_buf.insts_cycle_counts.push(inst_info.cycle as u16);
                assert!(asm.jit_buf.insts_cycle_counts.len() <= u16::MAX as usize, "{CPU:?} {guest_pc:x} {inst_info:?}")
            }

            // let is_unreturnable_branch = !inst_info.out_regs.is_reserved(Reg::LR) && inst_info.is_uncond_branch() && !inst_info.op.is_labelled_branch();
            let is_uncond_branch = inst_info.is_uncond_branch();
            let is_unknown = inst_info.op == Op::UnkArm || inst_info.op == Op::UnkThumb;

            asm.jit_buf.insts.push(inst_info);

            index += if THUMB { 2 } else { 4 };
            if is_uncond_branch || is_unknown {
                break;
            }
        }
    }

    // unsafe { BLOCK_LOG = true };

    let thread_regs = get_regs!(asm.emu, CPU);
    let mut block_asm = unsafe { (*asm.block_asm_buf.get()).new_asm(thread_regs, ptr::addr_of_mut!(asm.host_sp) as _) };

    for i in 0..asm.jit_buf.insts.len() {
        asm.jit_buf.current_index = i;
        asm.jit_buf.current_pc = guest_pc + (i << if THUMB { 1 } else { 2 }) as u32;
        debug_println!("{CPU:?} emitting {:?} at pc: {:x}", asm.jit_buf.current_inst(), asm.jit_buf.current_pc);

        if THUMB {
            asm.emit_thumb(&mut block_asm);
        } else {
            asm.emit(&mut block_asm);
        }

        // if DEBUG_LOG {
        //     block_asm.save_context();
        //     block_asm.call2(debug_after_exec_op::<CPU> as *const (), self.jit_buf.current_pc, self.jit_buf.current_inst().opcode);
        //     block_asm.restore_reg(Reg::CPSR);
        // }
    }

    let opcodes = block_asm.finalize(guest_pc, THUMB);
    if unsafe { BLOCK_LOG } {
        for &opcode in &opcodes {
            println!("0x{opcode:x},");
        }
    }
    let insert_entry = get_jit_mut!(asm.emu).insert_block::<CPU>(&opcodes, guest_pc);
    let jit_entry: extern "C" fn() = unsafe { mem::transmute(insert_entry) };

    if DEBUG_LOG {
        println!("{CPU:?} Mapping {guest_pc:#010x} to {:#010x}", jit_entry as *const fn() as usize);
    }
    asm.jit_buf.clear_all();

    jit_entry();
}

#[inline]
fn execute_internal<const CPU: CpuType>(guest_pc: u32) -> u16 {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut().unwrap_unchecked() };

    let thumb = (guest_pc & 1) == 1;
    debug_println!("{:?} Execute {:x} thumb {}", CPU, guest_pc, thumb);

    get_regs_mut!(asm.emu, CPU).set_thumb(thumb);

    let guest_pc = if thumb { guest_pc & !1 } else { guest_pc & !3 };
    let jit_entry = get_jit!(asm.emu).get_jit_start_addr::<CPU>(guest_pc);
    let jit_entry: extern "C" fn() = unsafe { mem::transmute(jit_entry) };

    debug_println!("{CPU:?} Enter jit addr {:x}", jit_entry as usize);

    if DEBUG_LOG {
        asm.runtime_data.branch_out_pc = u32::MAX;
        asm.runtime_data.branch_out_total_cycles = 0;
    }
    asm.runtime_data.pre_cycle_count_sum = 0;
    asm.runtime_data.accumulated_cycles = 0;
    get_regs_mut!(asm.emu, CPU).cycle_correction = 0;

    jit_entry();

    if DEBUG_LOG {
        assert_ne!(asm.runtime_data.branch_out_pc, u32::MAX);
        assert_ne!(asm.runtime_data.branch_out_total_cycles, 0);
    }

    let executed_cycles = (asm.runtime_data.branch_out_total_cycles
        - asm.runtime_data.pre_cycle_count_sum + asm.runtime_data.accumulated_cycles
        // + 2 for branching out
        + 2) as i16
        + get_regs_mut!(asm.emu, CPU).cycle_correction;

    if DEBUG_LOG && DEBUG_LOG_BRANCH_OUT {
        println!("{:?} reading opcode of breakout at {:x} executed cycles {executed_cycles}", CPU, asm.runtime_data.branch_out_pc);
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

    executed_cycles as u16
}

pub struct JitAsm<'a, const CPU: CpuType> {
    pub emu: &'a mut Emu,
    pub jit_buf: JitBuf,
    pub runtime_data: JitRuntimeData,
    pub block_asm_buf: UnsafeCell<BlockAsmBuf>,
    pub host_sp: u32,
}

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    #[inline(never)]
    pub fn new(emu: &'a mut Emu) -> Self {
        JitAsm {
            emu,
            jit_buf: JitBuf::new(),
            runtime_data: JitRuntimeData::new(),
            block_asm_buf: UnsafeCell::new(BlockAsmBuf::new()),
            host_sp: 0,
        }
    }

    #[inline(always)]
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

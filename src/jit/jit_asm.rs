use crate::core::emu::{get_jit, get_jit_mut, get_mem_mut, get_regs, get_regs_mut, Emu};
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::jit::assembler::block_asm::BLOCK_LOG;
use crate::jit::assembler::BlockAsmBuf;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_memory::JitInsertArgs;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::reg::{reg_reserve, RegReserve};
use crate::logging::debug_println;
use crate::{get_jit_asm_ptr, DEBUG_LOG, DEBUG_LOG_BRANCH_OUT};
use std::arch::asm;
use std::cell::UnsafeCell;
use std::intrinsics::likely;
use std::ptr;

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

    #[inline(never)]
    fn emit_code_block<const THUMB: bool>(&mut self, guest_pc: u32) {
        {
            let mut index = 0;
            loop {
                let inst_info = if THUMB {
                    let opcode = self.emu.mem_read::<CPU, u16>(guest_pc + index);
                    let (op, func) = lookup_thumb_opcode(opcode);
                    InstInfo::from(func(opcode, *op))
                } else {
                    let opcode = self.emu.mem_read::<CPU, u32>(guest_pc + index);
                    let (op, func) = lookup_opcode(opcode);
                    func(opcode, *op)
                };

                if let Some(last) = self.jit_buf.insts_cycle_counts.last() {
                    assert!(u16::MAX - last >= inst_info.cycle as u16, "{CPU:?} {guest_pc:x} {inst_info:?}");
                    self.jit_buf.insts_cycle_counts.push(last + inst_info.cycle as u16);
                } else {
                    self.jit_buf.insts_cycle_counts.push(inst_info.cycle as u16);
                    assert!(self.jit_buf.insts_cycle_counts.len() <= u16::MAX as usize, "{CPU:?} {guest_pc:x} {inst_info:?}")
                }

                // let is_unreturnable_branch = !inst_info.out_regs.is_reserved(Reg::LR) && inst_info.is_uncond_branch() && !inst_info.op.is_labelled_branch();
                let is_uncond_branch = inst_info.is_uncond_branch();
                let is_unknown = inst_info.op == Op::UnkArm || inst_info.op == Op::UnkThumb;

                self.jit_buf.insts.push(inst_info);

                index += if THUMB { 2 } else { 4 };
                if is_uncond_branch || is_unknown {
                    break;
                }
            }
        }

        // unsafe { BLOCK_LOG = guest_pc == 0x238015c };

        let thread_regs = get_regs!(self.emu, CPU);
        let mut block_asm = unsafe { (*self.block_asm_buf.get()).new_asm(thread_regs) };

        for i in 0..self.jit_buf.insts.len() {
            self.jit_buf.current_index = i;
            self.jit_buf.current_pc = guest_pc + (i << if THUMB { 1 } else { 2 }) as u32;
            debug_println!("{CPU:?} emitting {:?} at pc: {:x}", self.jit_buf.current_inst(), self.jit_buf.current_pc);

            if THUMB {
                self.emit_thumb(&mut block_asm);
            } else {
                self.emit(&mut block_asm);
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
            todo!();
        }
        get_jit_mut!(self.emu).insert_block::<CPU, THUMB>(&opcodes, JitInsertArgs::new(guest_pc, self.jit_buf.insts_cycle_counts.clone()));

        if DEBUG_LOG {
            let (jit_addr, _) = get_jit!(self.emu).get_jit_start_addr::<CPU, THUMB>(guest_pc).unwrap();
            println!("{:?} Mapping {:#010x} to {:#010x}", CPU, guest_pc, jit_addr);
        }
        self.jit_buf.clear_all();
    }

    #[inline(always)]
    pub fn execute(&mut self) -> u16 {
        let entry = get_regs!(self.emu, CPU).pc;

        let thumb = (entry & 1) == 1;
        debug_println!("{:?} Execute {:x} thumb {}", CPU, entry, thumb);
        if thumb {
            self.execute_internal::<true>(entry & !1)
        } else {
            self.execute_internal::<false>(entry & !3)
        }
    }

    #[inline]
    fn execute_internal<const THUMB: bool>(&mut self, guest_pc: u32) -> u16 {
        get_regs_mut!(self.emu, CPU).set_thumb(THUMB);

        let (jit_addr, jit_block_addr) = {
            let jit_info = get_jit!(self.emu).get_jit_start_addr::<CPU, THUMB>(guest_pc);
            if likely(jit_info.is_some()) {
                unsafe { jit_info.unwrap_unchecked() }
            } else {
                self.emit_code_block::<THUMB>(guest_pc);
                get_jit!(self.emu).get_jit_start_addr::<CPU, THUMB>(guest_pc).unwrap()
            }
        };

        debug_println!("{:?} Enter jit addr {:x}", CPU, jit_addr);

        if DEBUG_LOG {
            self.runtime_data.branch_out_pc = u32::MAX;
            self.runtime_data.branch_out_total_cycles = 0;
        }
        let (pre_cycle_count_sum, _) = get_jit!(self.emu).get_cycle_counts_unchecked::<THUMB>(guest_pc, jit_block_addr);
        self.runtime_data.pre_cycle_count_sum = pre_cycle_count_sum;
        self.runtime_data.accumulated_cycles = 0;

        get_mem_mut!(self.emu).current_jit_block_addr = jit_block_addr;

        unsafe { enter_jit(jit_addr, ptr::addr_of_mut!(self.host_sp) as u32, get_regs!(self.emu, CPU).cpsr) };

        if DEBUG_LOG {
            assert_ne!(self.runtime_data.branch_out_pc, u32::MAX);
            assert_ne!(self.runtime_data.branch_out_total_cycles, 0);
        }

        get_mem_mut!(self.emu).current_jit_block_addr = 0;

        let cycle_correction = {
            let regs = get_regs_mut!(self.emu, CPU);
            let correction = regs.cycle_correction;
            regs.cycle_correction = 0;
            correction
        };

        let executed_cycles = (self.runtime_data.branch_out_total_cycles
            - self.runtime_data.pre_cycle_count_sum + self.runtime_data.accumulated_cycles
            // + 2 for branching out
            + 2) as i16
            + cycle_correction;

        if DEBUG_LOG && DEBUG_LOG_BRANCH_OUT {
            println!("{:?} reading opcode of breakout at {:x} executed cycles {executed_cycles}", CPU, self.runtime_data.branch_out_pc);
            let inst_info = if THUMB {
                let opcode = self.emu.mem_read::<CPU, _>(self.runtime_data.branch_out_pc);
                let (op, func) = lookup_thumb_opcode(opcode);
                InstInfo::from(func(opcode, *op))
            } else {
                let opcode = self.emu.mem_read::<CPU, _>(self.runtime_data.branch_out_pc);
                let (op, func) = lookup_opcode(opcode);
                func(opcode, *op)
            };
            debug_inst_info::<CPU>(get_regs!(self.emu, CPU), self.runtime_data.branch_out_pc, &format!("breakout\n\t{:?} {:?}", CPU, inst_info));
        }

        executed_cycles as u16
    }
}

#[naked]
unsafe extern "C" fn enter_jit(jit_entry: u32, host_sp_ptr: u32, guest_cpsr: u32) {
    #[rustfmt::skip]
    asm!(
        "push {{r4-r11,lr}}",
        "str sp, [r1]",
        "msr cpsr, r2",
        "blx r0",
        "pop {{r4-r11,pc}}",
        options(noreturn)
    );
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

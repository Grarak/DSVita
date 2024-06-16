use crate::emu::emu::{get_jit, get_jit_mut, get_mem_mut, get_regs, get_regs_mut, Emu};
use crate::emu::thread_regs::ThreadRegs;
use crate::emu::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, Msr};
use crate::jit::assembler::arm::transfer_assembler::{LdrStrImm, LdrStrImmSBHD};
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::emitter::emit_branch::LOCAL_BRANCH_INDICATOR;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_memory::{JitInsertArgs, JIT_BLOCK_SIZE};
use crate::jit::reg::Reg;
use crate::jit::reg::{reg_reserve, RegReserve};
use crate::jit::Cond;
use crate::logging::debug_println;
use crate::{DEBUG_LOG, DEBUG_LOG_BRANCH_OUT};
use std::arch::asm;
use std::intrinsics::likely;
use std::ptr;

#[derive(Default)]
struct DebugRegs {
    gp: [u32; 13],
    sp: u32,
}

impl DebugRegs {
    fn emit_save_regs(&self) -> Vec<u32> {
        let last_gp_reg_addr = ptr::addr_of!(self.gp[self.gp.len() - 1]) as u32;
        let mut opcodes = Vec::new();
        opcodes.extend(AluImm::mov32(Reg::LR, last_gp_reg_addr));
        opcodes.extend([LdrStrImm::str_offset_al(Reg::SP, Reg::LR, 4), LdmStm::push_post(RegReserve::gp(), Reg::LR, Cond::AL)]);
        opcodes
    }

    fn emit_restore_regs(&self, thread_regs: &ThreadRegs) -> Vec<u32> {
        let gp_reg_addr = ptr::addr_of!(self.gp[0]) as u32;
        let cpsr_addr = thread_regs.get_reg(Reg::CPSR) as *const _ as u32;
        let mut opcodes = Vec::new();

        opcodes.extend(AluImm::mov32(Reg::SP, cpsr_addr));
        opcodes.push(LdrStrImm::ldr_al(Reg::R0, Reg::SP));
        opcodes.push(Msr::cpsr_flags(Reg::R0, Cond::AL));

        opcodes.extend(AluImm::mov32(Reg::SP, gp_reg_addr));
        opcodes.extend([LdmStm::pop_post_al(RegReserve::gp()), LdrStrImm::ldr_al(Reg::SP, Reg::SP)]);
        opcodes
    }
}

#[derive(Default)]
#[repr(C)]
pub struct HostRegs {
    sp: u32,
    lr: u32,
}

impl HostRegs {
    fn get_sp_addr(&self) -> u32 {
        ptr::addr_of!(self.sp) as _
    }

    fn get_lr_addr(&self) -> u32 {
        ptr::addr_of!(self.lr) as _
    }

    fn emit_restore_sp(&self) -> Vec<u32> {
        let mut opcodes = Vec::new();
        opcodes.extend(AluImm::mov32(Reg::LR, self.get_sp_addr()));
        opcodes.push(LdrStrImm::ldr_al(Reg::SP, Reg::LR));
        opcodes
    }
}

pub struct JitBuf {
    pub insts: Vec<InstInfo>,
    pub emit_opcodes: Vec<u32>,
    pub block_opcodes: Vec<u32>,
    pub jit_addr_offsets: Vec<u16>,
    pub insts_cycle_counts: Vec<u16>,
    pub local_branches: Vec<(u32, u32)>,
}

impl JitBuf {
    fn new() -> Self {
        JitBuf {
            insts: Vec::new(),
            emit_opcodes: Vec::new(),
            block_opcodes: Vec::new(),
            jit_addr_offsets: Vec::new(),
            insts_cycle_counts: Vec::new(),
            local_branches: Vec::new(),
        }
    }

    fn clear_all(&mut self) {
        self.insts.clear();
        self.block_opcodes.clear();
        self.jit_addr_offsets.clear();
        self.insts_cycle_counts.clear();
        self.local_branches.clear();
    }
}

#[repr(C)]
pub struct JitRuntimeData {
    pub branch_out_pc: u32,
    pub branch_out_total_cycles: u16,
    pub pre_cycle_count_sum: u16,
    pub accumulated_cycles: u16,
    pub next_event_in_cycles: u16,
    pub idle_loop: bool,
}

impl JitRuntimeData {
    fn new() -> Self {
        let instance = JitRuntimeData {
            branch_out_pc: u32::MAX,
            branch_out_total_cycles: 0,
            pre_cycle_count_sum: 0,
            accumulated_cycles: 0,
            next_event_in_cycles: 0,
            idle_loop: false,
        };
        let branch_out_pc_ptr = ptr::addr_of!(instance.branch_out_pc) as u32;
        let branch_out_total_cycles_ptr = ptr::addr_of!(instance.branch_out_total_cycles) as u32;
        let idle_loop_ptr = ptr::addr_of!(instance.idle_loop) as u32;
        assert_eq!(branch_out_total_cycles_ptr - branch_out_pc_ptr, 4);
        assert_eq!(idle_loop_ptr - branch_out_total_cycles_ptr, 8);
        instance
    }

    pub fn get_branch_out_addr(&self) -> *const u32 {
        ptr::addr_of!(self.branch_out_pc)
    }

    pub fn emit_get_branch_out_addr(&self, dest: Reg) -> Vec<u32> {
        AluImm::mov32(dest, self.get_branch_out_addr() as u32)
    }

    pub const fn get_total_cycles_offset() -> u8 {
        4
    }

    pub const fn get_idle_loop_offset() -> u8 {
        Self::get_total_cycles_offset() + 8
    }
}

pub struct JitAsm<'a, const CPU: CpuType> {
    pub emu: &'a mut Emu,
    pub jit_buf: JitBuf,
    pub host_regs: Box<HostRegs>,
    pub runtime_data: JitRuntimeData,
    pub breakin_addr: u32,
    pub breakout_addr: u32,
    pub breakout_skip_save_regs_addr: u32,
    pub breakin_thumb_addr: u32,
    pub breakout_thumb_addr: u32,
    pub breakout_skip_save_regs_thumb_addr: u32,
    pub restore_host_opcodes: Vec<u32>,
    pub restore_guest_opcodes: Vec<u32>,
    pub restore_host_thumb_opcodes: Vec<u32>,
    pub restore_guest_thumb_opcodes: Vec<u32>,
    debug_regs: DebugRegs,
}

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    #[inline(never)]
    pub fn new(emu: &'a mut Emu) -> Self {
        let mut instance = {
            let host_regs = Box::<HostRegs>::default();

            let restore_host_opcodes = {
                let mut opcodes = Vec::new();
                // Save guest
                opcodes.extend(&get_regs!(emu, CPU).save_regs_opcodes);
                // Restore host sp
                opcodes.extend(host_regs.emit_restore_sp());
                opcodes.shrink_to_fit();
                opcodes
            };

            let restore_guest_opcodes = {
                let mut opcodes = Vec::new();
                // Restore guest
                opcodes.push(AluShiftImm::mov_al(Reg::LR, Reg::SP));
                opcodes.extend(&get_regs!(emu, CPU).restore_regs_opcodes);
                opcodes.shrink_to_fit();
                opcodes
            };

            let restore_host_thumb_opcodes = {
                let mut opcodes = Vec::new();
                // Save guest
                opcodes.extend(&get_regs!(emu, CPU).save_regs_thumb_opcodes);

                // Restore host sp
                opcodes.extend(host_regs.emit_restore_sp());
                opcodes.shrink_to_fit();
                opcodes
            };

            let restore_guest_thumb_opcodes = {
                let mut opcodes = Vec::new();
                // Restore guest
                opcodes.push(AluShiftImm::mov_al(Reg::LR, Reg::SP));
                opcodes.extend(&get_regs!(emu, CPU).restore_regs_thumb_opcodes);
                opcodes.shrink_to_fit();
                opcodes
            };

            JitAsm {
                emu,
                jit_buf: JitBuf::new(),
                host_regs,
                runtime_data: JitRuntimeData::new(),
                breakin_addr: 0,
                breakout_addr: 0,
                breakout_skip_save_regs_addr: 0,
                breakin_thumb_addr: 0,
                breakout_thumb_addr: 0,
                breakout_skip_save_regs_thumb_addr: 0,
                restore_host_opcodes,
                restore_guest_opcodes,
                restore_host_thumb_opcodes,
                restore_guest_thumb_opcodes,
                debug_regs: DebugRegs::default(),
            }
        };

        get_jit_mut!(instance.emu).open();
        {
            let jit_opcodes = &mut instance.jit_buf.emit_opcodes;
            {
                // Common function to enter guest (breakin)
                // Save lr to return to this function
                jit_opcodes.extend(&AluImm::mov32(Reg::R1, instance.host_regs.get_lr_addr()));
                jit_opcodes.extend(&[
                    LdrStrImm::str(4, Reg::R0, Reg::SP, false, false, false, true, Cond::AL), // Save actual function entry
                    LdrStrImm::str_al(Reg::LR, Reg::R1),                                      // Save host lr
                    AluShiftImm::mov_al(Reg::LR, Reg::SP),                                    // Keep host sp in lr
                ]);
                // Restore guest
                let guest_restore_index = jit_opcodes.len();
                jit_opcodes.extend(&get_regs!(instance.emu, CPU).restore_regs_opcodes);

                jit_opcodes.push(LdrStrImm::ldr_sub_offset_al(Reg::PC, Reg::LR, 4));

                instance.breakin_addr = get_jit_mut!(instance.emu).insert_common(jit_opcodes);

                let restore_regs_thumb_opcodes = &get_regs!(instance.emu, CPU).restore_regs_thumb_opcodes;
                jit_opcodes[guest_restore_index..guest_restore_index + restore_regs_thumb_opcodes.len()].copy_from_slice(restore_regs_thumb_opcodes);
                instance.breakin_thumb_addr = get_jit_mut!(instance.emu).insert_common(jit_opcodes);

                jit_opcodes.clear();
            }

            {
                // Common function to exit guest (breakout)
                // Save guest
                jit_opcodes.extend(&get_regs!(instance.emu, CPU).save_regs_opcodes);

                // Some emits already save regs themselves, add skip addr
                let jit_skip_save_regs_offset = jit_opcodes.len() as u32;

                let host_sp_addr = instance.host_regs.get_sp_addr();
                jit_opcodes.extend(&AluImm::mov32(Reg::R0, host_sp_addr));
                jit_opcodes.extend(&[
                    LdrStrImm::ldr_al(Reg::SP, Reg::R0),           // Restore host SP
                    LdrStrImm::ldr_offset_al(Reg::PC, Reg::R0, 4), // Restore host LR and write to PC
                ]);

                instance.breakout_addr = get_jit_mut!(instance.emu).insert_common(jit_opcodes);
                instance.breakout_skip_save_regs_addr = instance.breakout_addr + (jit_skip_save_regs_offset << 2);

                jit_opcodes.clear();
            }

            {
                // Common thumb function to exit guest (breakout)
                // Save guest
                jit_opcodes.extend(&get_regs!(instance.emu, CPU).save_regs_thumb_opcodes);

                // Some emits already save regs themselves, add skip addr
                let jit_skip_save_regs_offset = jit_opcodes.len() as u32;

                let host_sp_addr = instance.host_regs.get_sp_addr();
                jit_opcodes.extend(&AluImm::mov32(Reg::R0, host_sp_addr));
                jit_opcodes.extend(&[
                    LdrStrImm::ldr_al(Reg::SP, Reg::R0),           // Restore host SP
                    LdrStrImm::ldr_offset_al(Reg::PC, Reg::R0, 4), // Restore host LR and write to PC
                ]);

                instance.breakout_thumb_addr = get_jit_mut!(instance.emu).insert_common(jit_opcodes);
                instance.breakout_skip_save_regs_thumb_addr = instance.breakout_thumb_addr + (jit_skip_save_regs_offset << 2);

                jit_opcodes.clear();
            }
        }
        get_jit_mut!(instance.emu).close();

        instance
    }

    #[inline(never)]
    fn emit_code_block<const THUMB: bool>(&mut self, guest_pc_block_base: u32) {
        {
            let mut index = 0;
            loop {
                let inst_info = if THUMB {
                    let opcode = self.emu.mem_read::<CPU, u16>(guest_pc_block_base + index);
                    debug_println!("{:?} disassemble thumb {:x} {}", CPU, opcode, opcode);
                    let (op, func) = lookup_thumb_opcode(opcode);
                    InstInfo::from(func(opcode, *op))
                } else {
                    let opcode = self.emu.mem_read::<CPU, u32>(guest_pc_block_base + index);
                    debug_println!("{:?} disassemble {:x} {}", CPU, opcode, opcode);
                    let (op, func) = lookup_opcode(opcode);
                    func(opcode, *op)
                };

                if let Some(last) = self.jit_buf.insts_cycle_counts.last() {
                    assert!(u16::MAX - last >= inst_info.cycle as u16);
                    self.jit_buf.insts_cycle_counts.push(last + inst_info.cycle as u16);
                } else {
                    self.jit_buf.insts_cycle_counts.push(inst_info.cycle as u16);
                }
                self.jit_buf.insts.push(inst_info);

                index += if THUMB { 2 } else { 4 };
                if index == JIT_BLOCK_SIZE {
                    break;
                }
            }
        }

        for i in 0..self.jit_buf.insts.len() {
            let pc = ((i as u32) << if THUMB { 1 } else { 2 }) + guest_pc_block_base;
            let opcodes_len = self.jit_buf.block_opcodes.len();
            assert!((opcodes_len << 2) <= u16::MAX as usize);
            self.jit_buf.jit_addr_offsets.push((opcodes_len << 2) as u16);

            debug_println!("{:?} emitting {:?}", CPU, self.jit_buf.insts[i]);

            if THUMB {
                self.emit_thumb(i, pc);
            } else {
                self.emit(i, pc);
            };

            if DEBUG_LOG {
                let inst_info = &self.jit_buf.insts[i];

                let jit_asm_addr = self as *const _ as u32;
                let opcodes = &mut self.jit_buf.emit_opcodes;

                opcodes.extend(self.debug_regs.emit_save_regs());
                opcodes.extend(self.host_regs.emit_restore_sp());

                opcodes.extend(AluImm::mov32(Reg::R0, jit_asm_addr));
                opcodes.extend(AluImm::mov32(Reg::R1, pc));
                opcodes.extend(AluImm::mov32(Reg::R2, inst_info.opcode));

                Self::emit_host_blx(debug_after_exec_op::<CPU> as *const () as _, opcodes);

                opcodes.push(AluShiftImm::mov_al(Reg::LR, Reg::SP));
                opcodes.extend(self.debug_regs.emit_restore_regs(get_regs!(self.emu, CPU)));
            }

            self.jit_buf.block_opcodes.extend(&self.jit_buf.emit_opcodes);
            self.jit_buf.emit_opcodes.clear();
        }

        {
            let opcodes = &mut self.jit_buf.block_opcodes;

            let regs = get_regs_mut!(self.emu, CPU);
            if THUMB {
                opcodes.extend(&regs.save_regs_thumb_opcodes);
            } else {
                opcodes.extend(&regs.save_regs_opcodes);
            }

            let new_pc = guest_pc_block_base + JIT_BLOCK_SIZE;

            opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::R1));
            opcodes.push(AluImm::mov16_al(Reg::R4, *self.jit_buf.insts_cycle_counts.last().unwrap()));
            if DEBUG_LOG_BRANCH_OUT {
                opcodes.extend(&AluImm::mov32(Reg::R0, new_pc - if THUMB { 2 } else { 4 }));
                opcodes.push(LdrStrImm::str_al(Reg::R0, Reg::R1));
            }
            opcodes.push(LdrStrImmSBHD::strh_al(Reg::R4, Reg::R1, JitRuntimeData::get_total_cycles_offset()));

            if THUMB {
                opcodes.extend(&AluImm::mov32(Reg::R2, new_pc + 1));
            } else {
                opcodes.extend(&AluImm::mov32(Reg::R2, new_pc));
            }
            opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(Reg::PC, Reg::R2, Reg::R3));

            if THUMB {
                Self::emit_host_bx(self.breakout_skip_save_regs_thumb_addr, opcodes);
            } else {
                Self::emit_host_bx(self.breakout_skip_save_regs_addr, opcodes);
            }
        }

        for (pc, pc_to_branch) in &self.jit_buf.local_branches {
            let base_index = ((pc - guest_pc_block_base) >> if THUMB { 1 } else { 2 }) as usize;
            let branch_to_index = ((pc_to_branch - guest_pc_block_base) >> if THUMB { 1 } else { 2 }) as usize;
            let base_offset = (self.jit_buf.jit_addr_offsets[base_index] >> 2) as usize;
            let branch_to_offset = (self.jit_buf.jit_addr_offsets[branch_to_index] >> 2) as usize;

            let branch_inst_offset = self.jit_buf.block_opcodes[base_offset..].iter().position(|opcode| *opcode == LOCAL_BRANCH_INDICATOR).unwrap();

            let offset = branch_to_offset as i32 - (base_offset as i32 + branch_inst_offset as i32);

            self.jit_buf.block_opcodes[base_offset + branch_inst_offset] = B::b(offset - 2, Cond::AL);
        }

        get_jit_mut!(self.emu).insert_block::<CPU, THUMB>(
            &self.jit_buf.block_opcodes,
            JitInsertArgs::new(guest_pc_block_base, self.jit_buf.jit_addr_offsets.clone(), self.jit_buf.insts_cycle_counts.clone()),
        );

        if DEBUG_LOG {
            for (index, inst_info) in self.jit_buf.insts.iter().enumerate() {
                let pc = ((index as u32) << if THUMB { 1 } else { 2 }) + guest_pc_block_base;
                let (jit_addr, _) = get_jit!(self.emu).get_jit_start_addr::<CPU, THUMB>(pc).unwrap();

                println!("{:?} Mapping {:#010x} to {:#010x} {:?}", CPU, pc, jit_addr, inst_info);
            }
        }
        self.jit_buf.clear_all();
    }

    #[inline(always)]
    pub fn execute(&mut self, next_event_in_cycles: u16) -> u16 {
        let entry = get_regs!(self.emu, CPU).pc;
        self.runtime_data.next_event_in_cycles = next_event_in_cycles;

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
                let guest_pc_block_base = guest_pc & !(JIT_BLOCK_SIZE - 1);
                self.emit_code_block::<THUMB>(guest_pc_block_base);
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

        get_mem_mut!(self.emu).current_mode_is_thumb = THUMB;
        get_mem_mut!(self.emu).current_jit_block_addr = jit_block_addr;

        unsafe { enter_jit(jit_addr, self.host_regs.get_sp_addr(), if THUMB { self.breakin_thumb_addr } else { self.breakin_addr }) };

        if DEBUG_LOG {
            assert_ne!(self.runtime_data.branch_out_pc, u32::MAX);
            assert_ne!(self.runtime_data.branch_out_total_cycles, 0);
        }

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

        if DEBUG_LOG_BRANCH_OUT {
            println!("{:?} reading opcode of breakout at {:x}", CPU, self.runtime_data.branch_out_pc);
            let inst_info = if THUMB {
                let opcode = self.emu.mem_read::<CPU, _>(self.runtime_data.branch_out_pc);
                let (op, func) = lookup_thumb_opcode(opcode);
                InstInfo::from(func(opcode, *op))
            } else {
                let opcode = self.emu.mem_read::<CPU, _>(self.runtime_data.branch_out_pc);
                let (op, func) = lookup_opcode(opcode);
                func(opcode, *op)
            };
            debug_inst_info::<CPU>(None, get_regs!(self.emu, CPU), self.runtime_data.branch_out_pc, &format!("breakout\n\t{:?} {:?}", CPU, inst_info));
        }

        executed_cycles as u16
    }
}

#[naked]
unsafe extern "C" fn enter_jit(jit_entry: u32, host_sp_addr: u32, breakin_addr: u32) {
    asm!("push {{r4-r11,lr}}", "str sp, [r1]", "blx r2", "pop {{r4-r11,pc}}", options(noreturn));
}

fn debug_inst_info<const CPU: CpuType>(debug_regs: Option<&DebugRegs>, regs: &ThreadRegs, pc: u32, append: &str) {
    let mut output = "Executed ".to_owned();

    match debug_regs {
        Some(debug_regs) => {
            for reg in RegReserve::gp_thumb() {
                output += &format!("{:?}: {:x}, ", reg, debug_regs.gp[reg as usize]);
            }
            for reg in (!RegReserve::gp_thumb()).get_gp_regs() {
                output += &format!("{:?}: {:x}, ", reg, if regs.is_thumb() { *regs.get_reg(reg) } else { debug_regs.gp[reg as usize] });
            }
            output += &format!("{:?}: {:x}, ", Reg::SP, debug_regs.sp);
            for reg in reg_reserve!(Reg::LR, Reg::PC, Reg::CPSR, Reg::SPSR) {
                let value = if reg != Reg::PC { *regs.get_reg(reg) } else { pc };
                output += &format!("{:?}: {:x}, ", reg, value);
            }
        }
        None => {
            for reg in reg_reserve!(Reg::SP, Reg::LR, Reg::PC, Reg::CPSR, Reg::SPSR) + RegReserve::gp() {
                let value = if reg != Reg::PC { *regs.get_reg(reg) } else { pc };
                output += &format!("{:?}: {:x}, ", reg, value);
            }
        }
    }

    println!("{:?} {}{}", CPU, output, append);
}

unsafe extern "C" fn debug_after_exec_op<const CPU: CpuType>(asm: *mut JitAsm<CPU>, pc: u32, opcode: u32) {
    let asm = asm.as_mut().unwrap_unchecked();
    let inst_info = {
        if get_regs!(asm.emu, CPU).is_thumb() {
            let (op, func) = lookup_thumb_opcode(opcode as u16);
            InstInfo::from(func(opcode as u16, *op))
        } else {
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        }
    };

    debug_inst_info::<CPU>(Some(&asm.debug_regs), get_regs!(asm.emu, CPU), pc, &format!("\n\t{:?} {:?}", CPU, inst_info));
}

use crate::hle::bios_context::BiosContext;
use crate::hle::cp15_context::Cp15Context;
use crate::hle::cpu_regs::CpuRegs;
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, Msr};
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::inst_mem_handler::InstMemHandler;
use crate::jit::jit_memory::JitMemory;
use crate::jit::reg::Reg;
use crate::jit::reg::{reg_reserve, RegReserve};
use crate::jit::Cond;
use crate::logging::debug_println;
use crate::utils::NoHashMap;
use crate::DEBUG_LOG;
use std::arch::asm;
use std::cell::RefCell;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;
use std::rc::Rc;

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
        opcodes.extend([
            LdrStrImm::str_offset_al(Reg::SP, Reg::LR, 4),
            LdmStm::push_post(RegReserve::gp(), Reg::LR, Cond::AL),
        ]);
        opcodes
    }

    fn emit_restore_regs<const CPU: CpuType>(&self, thread_regs: &ThreadRegs<CPU>) -> Vec<u32> {
        let gp_reg_addr = ptr::addr_of!(self.gp[0]) as u32;
        let cpsr_addr = thread_regs.get_reg_value(Reg::CPSR) as *const _ as u32;
        let mut opcodes = Vec::new();

        opcodes.extend(AluImm::mov32(Reg::SP, cpsr_addr));
        opcodes.push(LdrStrImm::ldr_al(Reg::R0, Reg::SP));
        opcodes.push(Msr::cpsr_flags(Reg::R0, Cond::AL));

        opcodes.extend(AluImm::mov32(Reg::SP, gp_reg_addr));
        opcodes.extend([
            LdmStm::pop_post_al(RegReserve::gp()),
            LdrStrImm::ldr_al(Reg::SP, Reg::SP),
        ]);
        opcodes
    }
}

pub struct JitState {
    pub invalidated_addrs: Vec<u32>,
    #[cfg(debug_assertions)]
    pub current_block_range: (u32, u32),
}

impl JitState {
    pub fn new() -> Self {
        JitState {
            invalidated_addrs: Vec::new(),
            #[cfg(debug_assertions)]
            current_block_range: (0, 0),
        }
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
    pub instructions: Vec<InstInfo>,
    pub emit_opcodes: Vec<u32>,
    pub block_opcodes: Vec<u32>,
    pub jit_addr_mapping: NoHashMap<u16>,
    pub insts_cycle_counts: Vec<u8>,
    pub regs_saved_previously: bool,
    pub regs_saved: bool,
}

impl JitBuf {
    fn new() -> Self {
        JitBuf {
            instructions: Vec::new(),
            emit_opcodes: Vec::new(),
            block_opcodes: Vec::new(),
            jit_addr_mapping: NoHashMap::default(),
            insts_cycle_counts: Vec::new(),
            regs_saved_previously: false,
            regs_saved: false,
        }
    }

    fn clear_all(&mut self) {
        self.instructions.clear();
        self.block_opcodes.clear();
        self.jit_addr_mapping.clear();
        self.insts_cycle_counts.clear();
        self.regs_saved_previously = false;
    }
}

pub struct JitAsm<const CPU: CpuType> {
    pub jit_memory: Rc<RefCell<JitMemory>>,
    pub inst_mem_handler: InstMemHandler<CPU>,
    pub thread_regs: Rc<RefCell<ThreadRegs<CPU>>>,
    pub cpu_regs: Rc<CpuRegs<CPU>>,
    pub cp15_context: Rc<RefCell<Cp15Context>>,
    pub bios_context: Rc<RefCell<BiosContext<CPU>>>,
    pub mem_handler: Rc<MemHandler<CPU>>,
    pub jit_buf: JitBuf,
    pub host_regs: Box<HostRegs>,
    pub guest_branch_out_pc: u32,
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
    pub restore_host_no_save_opcodes: Vec<u32>,
    debug_regs: DebugRegs,
}

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn new(
        jit_memory: Rc<RefCell<JitMemory>>,
        thread_regs: Rc<RefCell<ThreadRegs<CPU>>>,
        cpu_regs: Rc<CpuRegs<CPU>>,
        cp15_context: Rc<RefCell<Cp15Context>>,
        bios_context: Rc<RefCell<BiosContext<CPU>>>,
        mem_handler: Rc<MemHandler<CPU>>,
    ) -> Self {
        let mut instance = {
            let host_regs = Box::<HostRegs>::default();

            let restore_host_opcodes = {
                let mut opcodes = Vec::new();
                // Save guest
                opcodes.extend(&thread_regs.borrow().save_regs_opcodes);
                // Restore host sp
                opcodes.extend(host_regs.emit_restore_sp());
                opcodes.shrink_to_fit();
                opcodes
            };

            let restore_guest_opcodes = {
                let mut opcodes = Vec::new();
                // Restore guest
                opcodes.push(AluShiftImm::mov_al(Reg::LR, Reg::SP));
                opcodes.extend(&thread_regs.borrow().restore_regs_opcodes);
                opcodes.shrink_to_fit();
                opcodes
            };

            let restore_host_thumb_opcodes = {
                let mut opcodes = Vec::new();
                // Save guest
                opcodes.extend(&thread_regs.borrow().save_regs_thumb_opcodes);

                // Restore host sp
                opcodes.extend(host_regs.emit_restore_sp());
                opcodes.shrink_to_fit();
                opcodes
            };

            let restore_guest_thumb_opcodes = {
                let mut opcodes = Vec::new();
                // Restore guest
                opcodes.push(AluShiftImm::mov_al(Reg::LR, Reg::SP));
                opcodes.extend(&thread_regs.borrow().restore_regs_thumb_opcodes);
                opcodes.shrink_to_fit();
                opcodes
            };

            let restore_host_no_save_opcodes = host_regs.emit_restore_sp();

            JitAsm {
                jit_memory: jit_memory.clone(),
                inst_mem_handler: InstMemHandler::new(thread_regs.clone(), mem_handler.clone()),
                thread_regs: thread_regs.clone(),
                cpu_regs: cpu_regs.clone(),
                cp15_context,
                bios_context,
                mem_handler,
                jit_buf: JitBuf::new(),
                host_regs,
                guest_branch_out_pc: 0,
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
                restore_host_no_save_opcodes,
                debug_regs: DebugRegs::default(),
            }
        };

        let mut jit_memory = jit_memory.borrow_mut();
        jit_memory.open();
        {
            let jit_opcodes = &mut instance.jit_buf.emit_opcodes;
            {
                // Common function to enter guest (breakin)
                // Save lr to return to this function
                jit_opcodes.extend(&AluImm::mov32(Reg::R1, instance.host_regs.get_lr_addr()));
                jit_opcodes.extend(&[
                    LdrStrImm::str(4, Reg::R0, Reg::SP, false, false, false, true, Cond::AL), // Save actual function entry
                    LdrStrImm::str_al(Reg::LR, Reg::R1), // Save host lr
                    AluShiftImm::mov_al(Reg::LR, Reg::SP), // Keep host sp in lr
                ]);
                // Restore guest
                let guest_restore_index = jit_opcodes.len();
                jit_opcodes.extend(&instance.thread_regs.borrow().restore_regs_opcodes);

                jit_opcodes.push(LdrStrImm::ldr(
                    4,
                    Reg::PC,
                    Reg::LR,
                    false,
                    false,
                    false,
                    true,
                    Cond::AL,
                ));

                instance.breakin_addr =
                    jit_memory.insert_block::<CPU>(jit_opcodes, None, None, None, None);

                let restore_regs_thumb_opcodes =
                    &instance.thread_regs.borrow().restore_regs_thumb_opcodes;
                jit_opcodes
                    [guest_restore_index..guest_restore_index + restore_regs_thumb_opcodes.len()]
                    .copy_from_slice(restore_regs_thumb_opcodes);
                instance.breakin_thumb_addr =
                    jit_memory.insert_block::<CPU>(jit_opcodes, None, None, None, None);

                jit_opcodes.clear();
            }

            {
                // Common function to exit guest (breakout)
                // Save guest
                jit_opcodes.extend(&instance.thread_regs.borrow().save_regs_opcodes);

                // Some emits already save regs themselves, add skip addr
                let jit_skip_save_regs_offset = jit_opcodes.len() as u32;

                let host_sp_addr = instance.host_regs.get_sp_addr();
                jit_opcodes.extend(&AluImm::mov32(Reg::R0, host_sp_addr));
                jit_opcodes.extend(&[
                    LdrStrImm::ldr_al(Reg::SP, Reg::R0),           // Restore host SP
                    LdrStrImm::ldr_offset_al(Reg::PC, Reg::R0, 4), // Restore host LR and write to PC
                ]);

                instance.breakout_addr =
                    jit_memory.insert_block::<CPU>(jit_opcodes, None, None, None, None);
                instance.breakout_skip_save_regs_addr =
                    instance.breakout_addr + (jit_skip_save_regs_offset << 2);

                let save_regs_thumb_opcodes =
                    &instance.thread_regs.borrow().save_regs_thumb_opcodes;
                jit_opcodes[..save_regs_thumb_opcodes.len()]
                    .copy_from_slice(save_regs_thumb_opcodes);
                instance.breakout_thumb_addr =
                    jit_memory.insert_block::<CPU>(jit_opcodes, None, None, None, None);
                instance.breakout_skip_save_regs_thumb_addr =
                    instance.breakout_thumb_addr + (jit_skip_save_regs_offset << 2);

                jit_opcodes.clear();
            }
        }
        jit_memory.close();

        instance
    }

    fn emit_code_block<const THUMB: bool>(&mut self, entry: u32, jit_memory: &mut JitMemory) {
        let aligned_entry = entry & !1;
        {
            let mut index = 0;
            if THUMB {
                loop {
                    let opcode = self.mem_handler.read::<u16>(entry + index);
                    debug_println!("{:?} disassemble thumb {:x} {}", CPU, opcode, opcode);
                    let (op, func) = lookup_thumb_opcode(opcode);
                    let inst_info = func(opcode, *op);

                    self.jit_buf.insts_cycle_counts.push(inst_info.cycle);
                    self.jit_buf.instructions.push(InstInfo::from(&inst_info));

                    if inst_info.op.is_uncond_branch_thumb() {
                        break;
                    }
                    index += 2;
                }
            } else {
                loop {
                    let opcode = self.mem_handler.read::<u32>(entry + index);
                    debug_println!("{:?} disassemble arm {:x} {}", CPU, opcode, opcode);
                    let (op, func) = lookup_opcode(opcode);
                    let inst_info = func(opcode, *op);
                    let is_uncond_branch = inst_info.op.is_branch() && inst_info.cond == Cond::AL;

                    self.jit_buf.insts_cycle_counts.push(inst_info.cycle);
                    self.jit_buf.instructions.push(inst_info);

                    if is_uncond_branch {
                        break;
                    }
                    index += 4;
                }
            }
        }

        let pc_step_size = if THUMB { 1 } else { 2 };
        for i in 0..self.jit_buf.instructions.len() {
            let pc = ((i as u32) << pc_step_size) + aligned_entry;
            let opcodes_len = self.jit_buf.block_opcodes.len();
            if opcodes_len > 0 {
                self.jit_buf
                    .jit_addr_mapping
                    .insert(pc, (opcodes_len << 2) as u16);
            }

            if DEBUG_LOG {
                debug_println!("{:?} emitting {:?}", CPU, self.jit_buf.instructions[i]);

                self.jit_buf
                    .emit_opcodes
                    .push(AluShiftImm::mov_al(Reg::R0, Reg::R0)); // NOP
            }
            if THUMB {
                self.emit_thumb(i, pc);
            } else {
                self.emit(i, pc);
            };

            if DEBUG_LOG {
                let inst_info = &self.jit_buf.instructions[i];

                self.jit_buf
                    .emit_opcodes
                    .extend(self.debug_regs.emit_save_regs());
                self.jit_buf
                    .emit_opcodes
                    .extend(self.host_regs.emit_restore_sp());

                self.jit_buf
                    .emit_opcodes
                    .extend(AluImm::mov32(Reg::R0, self as *const _ as u32));
                self.jit_buf.emit_opcodes.extend(AluImm::mov32(Reg::R1, pc));
                self.jit_buf
                    .emit_opcodes
                    .extend(AluImm::mov32(Reg::R2, inst_info.opcode));

                Self::emit_host_blx(
                    debug_after_exec_op::<CPU> as *const () as _,
                    &mut self.jit_buf.emit_opcodes,
                );

                self.jit_buf
                    .emit_opcodes
                    .push(AluShiftImm::mov_al(Reg::LR, Reg::SP));
                self.jit_buf.emit_opcodes.extend(
                    self.debug_regs
                        .emit_restore_regs(self.thread_regs.borrow().deref()),
                );
            }

            self.jit_buf
                .block_opcodes
                .extend(&self.jit_buf.emit_opcodes);
            self.jit_buf.emit_opcodes.clear();

            self.jit_buf.regs_saved_previously =
                self.jit_buf.regs_saved && self.jit_buf.instructions[i].cond == Cond::AL;
            self.jit_buf.regs_saved = false;
        }

        let guest_pc_end = entry + ((self.jit_buf.instructions.len() as u32) << pc_step_size);
        // TODO statically analyze generated insts

        {
            jit_memory.insert_block::<CPU>(
                &self.jit_buf.block_opcodes,
                Some(entry),
                Some(self.jit_buf.jit_addr_mapping.clone()),
                Some(self.jit_buf.insts_cycle_counts.clone()),
                Some(guest_pc_end),
            );

            if DEBUG_LOG {
                for (index, inst_info) in self.jit_buf.instructions.iter().enumerate() {
                    let pc = ((index as u32) << pc_step_size) + aligned_entry;
                    let info = jit_memory.get_jit_start_addr::<CPU>(pc).unwrap();

                    debug_println!(
                        "{:?} Mapping {:#010x} to {:#010x} {:?}",
                        CPU,
                        pc,
                        info.jit_addr,
                        inst_info
                    );
                }
            }
        }
        self.jit_buf.clear_all();
    }

    pub fn execute(&mut self) -> u16 {
        let entry = self.thread_regs.borrow().pc;

        debug_println!("{:?} Execute {:x}", CPU, entry);
        let thumb = (entry & 1) == 1;
        if thumb {
            self.execute_internal::<true>(entry)
        } else {
            self.execute_internal::<false>(entry)
        }
    }

    fn execute_internal<const THUMB: bool>(&mut self, guest_pc: u32) -> u16 {
        self.thread_regs.borrow_mut().set_thumb(THUMB);

        let jit_memory = self.jit_memory.clone();
        let mut jit_memory = jit_memory.borrow_mut();
        let jit_info = {
            {
                let mut jit_state = self.mem_handler.jit_state.borrow_mut();
                for addr in &jit_state.invalidated_addrs {
                    jit_memory.invalidate_block::<CPU>(*addr);
                }
                jit_state.invalidated_addrs.clear();
            }

            match jit_memory.get_jit_start_addr::<CPU>(guest_pc) {
                Some(info) => info,
                None => {
                    self.emit_code_block::<THUMB>(guest_pc, jit_memory.deref_mut());
                    jit_memory.get_jit_start_addr::<CPU>(guest_pc).unwrap()
                }
            }
        };

        #[cfg(debug_assertions)]
        {
            self.mem_handler.jit_state.borrow_mut().current_block_range =
                (guest_pc, jit_info.guest_pc_end);
            self.guest_branch_out_pc = 0;
        }
        self.bios_context.borrow_mut().cycle_correction = 0;

        unsafe {
            enter_jit(
                jit_info.jit_addr,
                self.host_regs.get_sp_addr(),
                if THUMB {
                    self.breakin_thumb_addr
                } else {
                    self.breakin_addr
                },
            )
        };
        debug_assert_ne!(self.guest_branch_out_pc, 0);

        let branch_out_pc = if THUMB {
            self.guest_branch_out_pc | 1
        } else {
            self.guest_branch_out_pc
        };
        let executed_insts = (branch_out_pc - guest_pc) >> if THUMB { 1 } else { 2 };
        let insts_cycle_count = &jit_info.guest_insts_cycle_counts
            [((guest_pc - jit_info.guest_pc_start) >> if THUMB { 1 } else { 2 }) as usize..];
        // TODO cycle correction for conds
        let executed_cycles = insts_cycle_count[0..=executed_insts as usize].iter().fold(
            self.bios_context.borrow().cycle_correction + 2, // + 2 for branching
            |sum, count| sum + *count as u16,
        );

        if DEBUG_LOG {
            debug_println!(
                "{:?} reading opcode of breakout at {:x}",
                CPU,
                self.guest_branch_out_pc
            );
            let inst_info = if THUMB {
                let opcode = self.mem_handler.read(self.guest_branch_out_pc);
                let (op, func) = lookup_thumb_opcode(opcode);
                InstInfo::from(&func(opcode, *op))
            } else {
                let opcode = self.mem_handler.read(self.guest_branch_out_pc);
                let (op, func) = lookup_opcode(opcode);
                func(opcode, *op)
            };
            debug_inst_info::<CPU>(
                RegReserve::gp(),
                None,
                self.thread_regs.borrow().deref(),
                self.guest_branch_out_pc,
                &format!("breakout\n\t{:?} {:?}", CPU, inst_info),
            );
        }

        executed_cycles
    }
}

#[naked]
unsafe extern "C" fn enter_jit(jit_entry: u32, host_sp_addr: u32, breakin_addr: u32) {
    asm!(
        "push {{r4-r11,lr}}",
        "str sp, [r1]",
        "blx r2",
        "pop {{r4-r11,pc}}",
        options(noreturn)
    );
}

fn debug_inst_info<const CPU: CpuType>(
    regs_to_log: RegReserve,
    debug_regs: Option<&DebugRegs>,
    regs: &ThreadRegs<CPU>,
    pc: u32,
    append: &str,
) {
    let mut output = "Executed ".to_owned();

    match debug_regs {
        Some(debug_regs) => {
            for reg in RegReserve::gp_thumb() & regs_to_log {
                output += &format!("{:?}: {:x}, ", reg, debug_regs.gp[reg as usize]);
            }
            for reg in ((!RegReserve::gp_thumb()).get_gp_regs()) & regs_to_log {
                output += &format!(
                    "{:?}: {:x}, ",
                    reg,
                    if regs.is_thumb() {
                        *regs.get_reg_value(reg)
                    } else {
                        debug_regs.gp[reg as usize]
                    }
                );
            }
            output += &format!("{:?}: {:x}, ", Reg::SP, debug_regs.sp);
            for reg in reg_reserve!(Reg::LR, Reg::PC, Reg::CPSR, Reg::SPSR) {
                let value = if reg != Reg::PC {
                    *regs.get_reg_value(reg)
                } else {
                    pc
                };
                output += &format!("{:?}: {:x}, ", reg, value);
            }
        }
        None => {
            for reg in reg_reserve!(Reg::SP, Reg::LR, Reg::PC, Reg::CPSR, Reg::SPSR) + regs_to_log {
                let value = if reg != Reg::PC {
                    *regs.get_reg_value(reg)
                } else {
                    pc
                };
                output += &format!("{:?}: {:x}, ", reg, value);
            }
        }
    }

    println!("{:?} {}{}", CPU, output, append);
}

unsafe extern "C" fn debug_after_exec_op<const CPU: CpuType>(
    asm: *const JitAsm<CPU>,
    pc: u32,
    opcode: u32,
) {
    let asm = asm.as_ref().unwrap();
    let inst_info = {
        if asm.thread_regs.borrow().is_thumb() {
            let (op, func) = lookup_thumb_opcode(opcode as u16);
            InstInfo::from(&func(opcode as u16, *op))
        } else {
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        }
    };

    let regs = asm.thread_regs.borrow();
    debug_inst_info::<CPU>(
        inst_info.src_regs + inst_info.out_regs,
        Some(&asm.debug_regs),
        regs.deref(),
        pc,
        &format!("\n\t{:?} {:?}", CPU, inst_info),
    );
}

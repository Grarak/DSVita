use crate::hle::bios_context::BiosContext;
use crate::hle::cp15_context::Cp15Context;
use crate::hle::cpu_regs::CpuRegs;
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::timers_context::TimersContext;
use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, Mrs};
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::inst_mem_handler::InstMemHandler;
use crate::jit::jit_cycle_handler::JitCycleManager;
use crate::jit::jit_memory::JitMemory;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use crate::logging::debug_println;
use crate::utils::FastCell;
use crate::DEBUG;
use std::arch::asm;
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::ptr;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct JitState {
    pub invalidated_addrs: HashSet<u32>,
    pub current_block_range: (u32, u32),
}

impl JitState {
    pub fn new() -> Self {
        JitState {
            invalidated_addrs: HashSet::new(),
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
    pub fn get_sp_addr(&self) -> u32 {
        ptr::addr_of!(self.sp) as _
    }

    pub fn get_lr_addr(&self) -> u32 {
        ptr::addr_of!(self.lr) as _
    }
}

pub struct JitBuf {
    pub instructions: Vec<InstInfo>,
    pub emit_opcodes: Vec<u32>,
    pub jit_addr_mapping: HashMap<u32, u16>,
    pub insts_cycle_counts: Vec<u8>,
}

impl JitBuf {
    fn new() -> Self {
        JitBuf {
            instructions: Vec::new(),
            emit_opcodes: Vec::new(),
            jit_addr_mapping: HashMap::new(),
            insts_cycle_counts: Vec::new(),
        }
    }

    fn clear_all(&mut self) {
        self.instructions.clear();
        self.emit_opcodes.clear();
        self.jit_addr_mapping.clear();
        self.insts_cycle_counts.clear();
    }
}

pub struct JitAsm {
    pub cpu_type: CpuType,
    jit_cycle_manager: Arc<RwLock<JitCycleManager>>,
    jit_memory: Arc<RwLock<JitMemory>>,
    pub inst_mem_handler: InstMemHandler,
    pub thread_regs: Rc<FastCell<ThreadRegs>>,
    pub cpu_regs: Arc<CpuRegs>,
    pub cp15_context: Rc<FastCell<Cp15Context>>,
    pub bios_context: BiosContext,
    timers_context: Arc<RwLock<TimersContext>>,
    pub mem_handler: Arc<MemHandler>,
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
}

impl JitAsm {
    pub fn new(
        cpu_type: CpuType,
        jit_cycle_manager: Arc<RwLock<JitCycleManager>>,
        jit_memory: Arc<RwLock<JitMemory>>,
        thread_regs: Rc<FastCell<ThreadRegs>>,
        cpu_regs: Arc<CpuRegs>,
        cp15_context: Rc<FastCell<Cp15Context>>,
        timers_context: Arc<RwLock<TimersContext>>,
        mem_handler: Arc<MemHandler>,
    ) -> Self {
        let mut instance = {
            let host_regs = Box::new(HostRegs::default());

            let restore_host_opcodes = {
                let mut opcodes = Vec::new();
                // Save guest
                opcodes.extend(&thread_regs.borrow().save_regs_opcodes);

                // Restore host sp
                let host_sp_addr = host_regs.get_sp_addr();
                opcodes.extend(AluImm::mov32(Reg::LR, host_sp_addr));
                opcodes.push(LdrStrImm::ldr_al(Reg::SP, Reg::LR));
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
                let host_sp_addr = host_regs.get_sp_addr();
                opcodes.extend(AluImm::mov32(Reg::LR, host_sp_addr));
                opcodes.push(LdrStrImm::ldr_al(Reg::SP, Reg::LR));
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

            JitAsm {
                cpu_type,
                jit_cycle_manager,
                jit_memory,
                inst_mem_handler: InstMemHandler::new(
                    cpu_type,
                    thread_regs.clone(),
                    mem_handler.clone(),
                ),
                thread_regs: thread_regs.clone(),
                cpu_regs: cpu_regs.clone(),
                cp15_context,
                bios_context: BiosContext::new(
                    cpu_type,
                    thread_regs,
                    cpu_regs,
                    mem_handler.clone(),
                ),
                timers_context,
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
            }
        };

        {
            let jit_opcodes = &mut instance.jit_buf.emit_opcodes;
            {
                // Common function to enter guest (breakin)
                // Save lr to return to this function
                jit_opcodes.extend(&AluImm::mov32(Reg::R1, instance.host_regs.get_lr_addr()));
                jit_opcodes.extend(&[
                    Mrs::cpsr(Reg::R2, Cond::AL),
                    LdmStm::push_pre(reg_reserve!(Reg::R0), Reg::SP, Cond::AL), // Save actual function entry
                    LdrStrImm::str_al(Reg::LR, Reg::R1),                        // Save host lr
                    LdrStrImm::str_offset_al(Reg::R2, Reg::R1, 4),              // Save host cpsr
                    AluShiftImm::mov_al(Reg::LR, Reg::SP), // Keep host sp in lr
                ]);
                // Restore guest
                let guest_restore_index = jit_opcodes.len();
                jit_opcodes.extend(&instance.thread_regs.borrow().restore_regs_opcodes);

                jit_opcodes.push(LdmStm::pop_post(reg_reserve!(Reg::PC), Reg::LR, Cond::AL));

                let mut jit_memory = instance.jit_memory.write().unwrap();
                instance.breakin_addr =
                    jit_memory.insert_block(&jit_opcodes, None, None, None, None);

                let restore_regs_thumb_opcodes =
                    &instance.thread_regs.borrow().restore_regs_thumb_opcodes;
                jit_opcodes
                    [guest_restore_index..guest_restore_index + restore_regs_thumb_opcodes.len()]
                    .copy_from_slice(restore_regs_thumb_opcodes);
                instance.breakin_thumb_addr =
                    jit_memory.insert_block(&jit_opcodes, None, None, None, None);

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
                    LdrStrImm::ldr_offset_al(Reg::LR, Reg::R0, 4), // Restore host LR
                    Bx::bx(Reg::LR, Cond::AL),
                ]);

                let mut jit_memory = instance.jit_memory.write().unwrap();
                instance.breakout_addr =
                    jit_memory.insert_block(&jit_opcodes, None, None, None, None);
                instance.breakout_skip_save_regs_addr =
                    instance.breakout_addr + jit_skip_save_regs_offset * 4;

                let save_regs_thumb_opcodes =
                    &instance.thread_regs.borrow().save_regs_thumb_opcodes;
                jit_opcodes[..save_regs_thumb_opcodes.len()]
                    .copy_from_slice(save_regs_thumb_opcodes);
                instance.breakout_thumb_addr =
                    jit_memory.insert_block(&jit_opcodes, None, None, None, None);
                instance.breakout_skip_save_regs_thumb_addr =
                    instance.breakout_thumb_addr + jit_skip_save_regs_offset * 4;

                jit_opcodes.clear();
            }
        }

        instance
    }

    fn emit_code_block<const THUMB: bool>(&mut self, entry: u32) -> (u32, u32, Vec<u8>) {
        {
            let mut index = 0;
            if THUMB {
                let mut buf = [0u16; 2048];
                'outer: loop {
                    let read_amount = self.mem_handler.read_slice(entry + index, &mut buf);
                    for opcode in &buf[..read_amount] {
                        debug_println!(
                            "{:?} disassemble thumb {:x} {}",
                            self.cpu_type,
                            opcode,
                            opcode
                        );
                        let (op, func) = lookup_thumb_opcode(*opcode);
                        let inst_info = func(*opcode, *op);

                        self.jit_buf.insts_cycle_counts.push(inst_info.cycle);
                        self.jit_buf.instructions.push(InstInfo::from(&inst_info));

                        if inst_info.op.is_unconditional_branch_thumb() {
                            break 'outer;
                        }
                    }
                    index += buf.len() as u32 * 2;
                }
            } else {
                let mut buf = [0u32; 1024];
                'outer: loop {
                    let read_amount = self.mem_handler.read_slice(entry + index, &mut buf);
                    for opcode in &buf[..read_amount] {
                        debug_println!(
                            "{:?} disassemble arm {:x} {}",
                            self.cpu_type,
                            opcode,
                            opcode
                        );
                        let (op, func) = lookup_opcode(*opcode);
                        let inst_info = func(*opcode, *op);
                        let is_branch = inst_info.op.is_branch();
                        let cond = inst_info.cond;

                        self.jit_buf.insts_cycle_counts.push(inst_info.cycle);
                        self.jit_buf.instructions.push(inst_info);

                        if is_branch && cond == Cond::AL {
                            break 'outer;
                        }
                    }
                    index += buf.len() as u32 * 4;
                }
            }
        }

        let pc_step_size = if THUMB { 2 } else { 4 };
        for i in 0..self.jit_buf.instructions.len() {
            let pc = i as u32 * pc_step_size + entry;
            let opcodes_len = self.jit_buf.emit_opcodes.len();
            if opcodes_len > 0 {
                self.jit_buf
                    .jit_addr_mapping
                    .insert(pc, (opcodes_len * 4) as u16);
            }

            if DEBUG {
                debug_println!(
                    "{:?} emitting {:?}",
                    self.cpu_type,
                    self.jit_buf.instructions[i]
                );

                self.jit_buf
                    .emit_opcodes
                    .push(AluShiftImm::mov_al(Reg::R0, Reg::R0)); // NOP
            }
            if THUMB {
                self.emit_thumb(i, pc);
            } else {
                self.emit(i, pc);
            };

            if DEBUG {
                let inst_info = &self.jit_buf.instructions[i];

                if (THUMB && !inst_info.op.is_unconditional_branch_thumb())
                    || (!THUMB && (!inst_info.op.is_branch() || inst_info.cond != Cond::AL))
                {
                    self.jit_buf.emit_opcodes.extend(&[
                        AluShiftImm::mov_al(Reg::R0, Reg::R0), // NOP
                        AluShiftImm::mov_al(Reg::R0, Reg::R0), // NOP
                    ]);

                    self.emit_call_host_func(
                        |_| {},
                        &[
                            Some(self as *const _ as u32),
                            Some(pc),
                            Some(inst_info.opcode),
                        ],
                        debug_after_exec_op as _,
                    );
                }
            }
        }

        let guest_pc_end = entry + self.jit_buf.instructions.len() as u32 * pc_step_size;
        // TODO statically analyze generated insts
        let addr = {
            let mut jit_memory = self.jit_memory.write().unwrap();

            let addr = jit_memory.insert_block(
                &self.jit_buf.emit_opcodes,
                Some(entry),
                Some(self.jit_buf.jit_addr_mapping.clone()),
                Some(self.jit_buf.insts_cycle_counts.clone()),
                Some(guest_pc_end),
            );

            if DEBUG {
                for (index, inst_info) in self.jit_buf.instructions.iter().enumerate() {
                    let pc = index as u32 * pc_step_size + entry;
                    let (jit_addr, _, _) = jit_memory.get_jit_start_addr::<THUMB>(pc).unwrap();

                    debug_println!(
                        "{:?} Mapping {:#010x} to {:#010x} {:?}",
                        self.cpu_type,
                        pc,
                        jit_addr,
                        inst_info
                    );
                }
            }

            addr
        };

        let cycle_count = self.jit_buf.insts_cycle_counts.clone();
        self.jit_buf.clear_all();
        (addr, guest_pc_end, cycle_count)
    }

    pub fn execute(&mut self) {
        let entry = self.thread_regs.borrow().pc;

        debug_println!("{:?} Execute {:x}", self.cpu_type, entry);
        self.execute_internal(entry);
    }

    fn execute_internal(&mut self, guest_pc: u32) {
        let thumb = (guest_pc & 1) == 1;
        let guest_pc = guest_pc & !1;

        self.thread_regs.borrow_mut().set_thumb(thumb);

        let (jit_entry, guest_pc_end, insts_cycle_count) = {
            {
                let mut jit_memory = self.jit_memory.write().unwrap();

                {
                    let mut jit_state = self.mem_handler.jit_state.lock().unwrap();
                    for addr in &jit_state.invalidated_addrs {
                        jit_memory.invalidate_block(*addr);
                    }
                    jit_state.invalidated_addrs.clear();
                }

                if thumb {
                    jit_memory.get_jit_start_addr::<true>(guest_pc)
                } else {
                    jit_memory.get_jit_start_addr::<false>(guest_pc)
                }
            }
            .unwrap_or_else(|| {
                if thumb {
                    self.emit_code_block::<true>(guest_pc)
                } else {
                    self.emit_code_block::<false>(guest_pc)
                }
            })
        };

        self.mem_handler
            .jit_state
            .lock()
            .unwrap()
            .current_block_range = (guest_pc, guest_pc_end);

        if DEBUG {
            self.guest_branch_out_pc = 0;
        }
        self.bios_context.cycle_correction = 0;

        // let now = std::time::Instant::now();
        unsafe {
            JitAsm::enter_jit(
                jit_entry,
                self.host_regs.get_sp_addr(),
                if thumb {
                    self.breakin_thumb_addr
                } else {
                    self.breakin_addr
                },
            )
        };
        // let elapsed_time = now.elapsed();
        debug_assert_ne!(self.guest_branch_out_pc, 0);

        let executed_insts = (self.guest_branch_out_pc - guest_pc) / (!thumb as u32 * 2 + 2);
        let executed_cycles = insts_cycle_count[0..=executed_insts as usize]
            .iter()
            .fold(0u16, |sum, count| sum + *count as u16)
            + self.bios_context.cycle_correction
            + 2; // + 2 for branching
        self.jit_cycle_manager
            .write()
            .unwrap()
            .on_cycle_update(self.cpu_type, executed_cycles);

        // TODO cycle correction for conds
        // self.jit_cycle_manager.write().unwrap().insert(
        //     self.cpu_type,
        //     elapsed_time,
        //     executed_cycles,
        // );

        if DEBUG {
            debug_println!(
                "{:?} reading opcode of breakout at {:x}",
                self.cpu_type,
                self.guest_branch_out_pc
            );
            let inst_info = if thumb {
                let opcode = self.mem_handler.read(self.guest_branch_out_pc);
                let (op, func) = lookup_thumb_opcode(opcode);
                InstInfo::from(&func(opcode, *op))
            } else {
                let opcode = self.mem_handler.read(self.guest_branch_out_pc);
                let (op, func) = lookup_opcode(opcode);
                func(opcode, *op)
            };
            debug_inst_info(
                self.cpu_type,
                RegReserve::gp(),
                self.thread_regs.borrow().deref(),
                self.guest_branch_out_pc,
                &format!("breakout\n\t{:?} {:?}", self.cpu_type, inst_info),
            );
        }
    }

    #[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
    #[inline(never)]
    unsafe extern "C" fn enter_jit(jit_entry: u32, host_sp_addr: u32, breakin_addr: u32) {
        asm!(
            "push {{r4-r12, lr}}",
            "mov r0, {jit_entry}",
            "mov r1, {host_sp_addr}",
            "mov r2, {breakin_addr}",
            "str sp, [r1]",
            "blx r2",
            "pop {{r4-r12, lr}}",
            jit_entry = in(reg) jit_entry,
            host_sp_addr = in(reg) host_sp_addr,
            breakin_addr = in(reg) breakin_addr,
        );
    }
}

fn debug_inst_info(
    cpu_type: CpuType,
    regs_to_log: RegReserve,
    regs: &ThreadRegs,
    pc: u32,
    append: &str,
) {
    let mut output = "Executed ".to_owned();
    for reg in reg_reserve!(Reg::SP, Reg::LR, Reg::PC, Reg::CPSR) + regs_to_log {
        let value = if reg != Reg::PC {
            *regs.get_reg_value(reg)
        } else {
            pc
        };
        output += &format!("{:?}: {:x}, ", reg, value);
    }
    debug_println!("{:?} {}{}", cpu_type, output, append);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
unsafe extern "C" fn debug_after_exec_op(asm: *const JitAsm, pc: u32, opcode: u32) {
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

    let regs = (*asm).thread_regs.borrow();
    debug_inst_info(
        (*asm).cpu_type,
        inst_info.src_regs + inst_info.out_regs,
        regs.deref(),
        pc,
        &format!("\n\t{:?} {:?}", asm.cpu_type, inst_info),
    );
}

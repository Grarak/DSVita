use crate::hle::cp15_context::Cp15Context;
use crate::hle::memory::handler::mem_handler::MemHandler;
use crate::hle::registers::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg};
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, Mrs};
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_memory::JitMemory;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use crate::logging::debug_println;
use crate::DEBUG;
use std::arch::asm;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::ptr;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

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
}

impl JitBuf {
    fn new() -> Self {
        JitBuf {
            instructions: Vec::new(),
            emit_opcodes: Vec::new(),
            jit_addr_mapping: HashMap::new(),
        }
    }

    fn clear_all(&mut self) {
        self.instructions.clear();
        self.emit_opcodes.clear();
        self.jit_addr_mapping.clear();
    }
}

pub struct JitAsm {
    jit_memory: Arc<RwLock<JitMemory>>,
    pub cpu_type: CpuType,
    pub mem_handler: Rc<RefCell<MemHandler>>,
    pub thread_regs: Rc<RefCell<ThreadRegs>>,
    pub cp15_context: Rc<RefCell<Cp15Context>>,
    pub jit_buf: JitBuf,
    pub host_regs: Box<HostRegs>,
    pub guest_branch_out_pc: u32,
    pub breakin_addr: u32,
    pub breakout_addr: u32,
    pub breakout_skip_save_regs_addr: u32,
    pub breakin_thumb_addr: u32,
    pub breakout_thumb_addr: u32,
    pub breakout_skip_save_regs_thumb_addr: u32,
    pub restore_host_opcodes: [u32; 7],
    pub restore_guest_opcodes: [u32; 7],
    pub restore_host_thumb_opcodes: [u32; 7],
    pub restore_guest_thumb_opcodes: [u32; 7],
}

impl JitAsm {
    pub fn new(
        jit_memory: Arc<RwLock<JitMemory>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        cp15_context: Rc<RefCell<Cp15Context>>,
        mem_handler: Rc<RefCell<MemHandler>>,
        cpu_type: CpuType,
    ) -> Self {
        let mut instance = {
            let host_regs = Box::new(HostRegs::default());

            let restore_host_opcodes = {
                let mut opcodes = [0u32; 7];
                // Save guest
                let save_regs_opcodes = &thread_regs.borrow().save_regs_opcodes;
                let index = save_regs_opcodes.len();
                opcodes[..index].copy_from_slice(save_regs_opcodes);

                // Restore host sp
                let host_sp_addr = host_regs.get_sp_addr();
                opcodes[index..index + 2].copy_from_slice(&AluImm::mov32(Reg::LR, host_sp_addr));
                opcodes[index + 2] = LdrStrImm::ldr_al(Reg::SP, Reg::LR); // SP
                opcodes
            };

            let restore_guest_opcodes = {
                let mut opcodes = [0u32; 7];
                // Restore guest
                opcodes[0] = AluReg::mov_al(Reg::LR, Reg::SP);
                opcodes[1..].copy_from_slice(&thread_regs.borrow().restore_regs_opcodes);
                opcodes
            };

            let restore_host_thumb_opcodes = {
                let mut opcodes = restore_host_opcodes;
                let save_regs_thumb_opcodes = &thread_regs.borrow().save_regs_thumb_opcodes;
                opcodes[..save_regs_thumb_opcodes.len()].copy_from_slice(save_regs_thumb_opcodes);
                opcodes
            };

            let restore_guest_thumb_opcodes = {
                let mut opcodes = restore_guest_opcodes;
                opcodes[1..].copy_from_slice(&thread_regs.borrow().restore_regs_thumb_opcodes);
                opcodes
            };

            JitAsm {
                jit_memory,
                cpu_type,
                mem_handler,
                thread_regs,
                cp15_context,
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
                jit_opcodes.extend(&AluImm::mov32(Reg::R0, instance.host_regs.get_lr_addr()));
                jit_opcodes.extend(&[
                    LdrStrImm::str_al(Reg::LR, Reg::R0), // Save host lr
                    Mrs::cpsr(Reg::LR, Cond::AL),
                    LdrStrImm::str_offset_al(Reg::LR, Reg::R0, 4), // Save host cpsr
                    AluReg::mov_al(Reg::LR, Reg::SP),              // Keep host sp in lr
                    LdmStm::push_pre(reg_reserve!(Reg::R4), Reg::LR, Cond::AL), // Save actual entry
                ]);
                // Restore guest
                let guest_restore_index = jit_opcodes.len();
                jit_opcodes.extend(&instance.thread_regs.borrow().restore_regs_opcodes);

                jit_opcodes.push(LdmStm::pop_post(reg_reserve!(Reg::PC), Reg::LR, Cond::AL));

                let mut jit_memory = instance.jit_memory.write().unwrap();
                instance.breakin_addr = jit_memory.insert_block(&jit_opcodes, None, None, None);

                let restore_regs_thumb_opcodes =
                    &instance.thread_regs.borrow().restore_regs_thumb_opcodes;
                jit_opcodes
                    [guest_restore_index..guest_restore_index + restore_regs_thumb_opcodes.len()]
                    .copy_from_slice(restore_regs_thumb_opcodes);
                instance.breakin_thumb_addr =
                    jit_memory.insert_block(&jit_opcodes, None, None, None);

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
                instance.breakout_addr = jit_memory.insert_block(&jit_opcodes, None, None, None);
                instance.breakout_skip_save_regs_addr =
                    instance.breakout_addr + jit_skip_save_regs_offset * 4;

                let save_regs_thumb_opcodes =
                    &instance.thread_regs.borrow().save_regs_thumb_opcodes;
                jit_opcodes[..save_regs_thumb_opcodes.len()]
                    .copy_from_slice(save_regs_thumb_opcodes);
                instance.breakout_thumb_addr =
                    jit_memory.insert_block(&jit_opcodes, None, None, None);
                instance.breakout_skip_save_regs_thumb_addr =
                    instance.breakout_thumb_addr + jit_skip_save_regs_offset * 4;

                jit_opcodes.clear();
            }
        }

        instance
    }

    fn emit_code_block(&mut self, entry: u32, thumb: bool) -> (u32, u32) {
        {
            let mem_handler = self.mem_handler.borrow();
            let mut index = 0;
            if thumb {
                let mut buf = [0u16; 2048];
                'outer: loop {
                    mem_handler.read_slice(entry + index, &mut buf);
                    for opcode in buf {
                        debug_println!(
                            "{:?} disassemble thumb {:x} {}",
                            self.cpu_type,
                            opcode,
                            opcode
                        );
                        let (op, func) = lookup_thumb_opcode(opcode);
                        let inst_info = func(opcode, *op);

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
                    mem_handler.read_slice(entry + index, &mut buf);
                    for opcode in buf {
                        debug_println!(
                            "{:?} disassemble arm {:x} {}",
                            self.cpu_type,
                            opcode,
                            opcode
                        );
                        let (op, func) = lookup_opcode(opcode);
                        let inst_info = func(opcode, *op);

                        self.jit_buf.instructions.push(inst_info);

                        if inst_info.op.is_branch() && inst_info.cond == Cond::AL {
                            break 'outer;
                        }
                    }
                    index += buf.len() as u32 * 4;
                }
            }
        }

        let pc_step_size = 2 + !thumb as u32 * 2;
        let emit_func = if thumb {
            JitAsm::emit_thumb
        } else {
            JitAsm::emit
        };
        for i in 0..self.jit_buf.instructions.len() {
            let pc = i as u32 * pc_step_size + entry;
            let opcodes_len = self.jit_buf.emit_opcodes.len();
            if opcodes_len > 0 {
                self.jit_buf
                    .jit_addr_mapping
                    .insert(pc, (opcodes_len * 4) as u16);
            }

            if DEBUG {
                debug_println!("Emitting {:?}", self.jit_buf.instructions[i]);

                self.jit_buf
                    .emit_opcodes
                    .push(AluReg::mov_al(Reg::R0, Reg::R0)); // NOP
            }
            emit_func(self, i, pc);

            if DEBUG {
                let inst_info = &self.jit_buf.instructions[i];

                if (thumb && !inst_info.op.is_unconditional_branch_thumb())
                    || (!thumb && (!inst_info.op.is_branch() || inst_info.cond != Cond::AL))
                {
                    self.jit_buf.emit_opcodes.extend(&[
                        AluReg::mov_al(Reg::R0, Reg::R0), // NOP
                        AluReg::mov_al(Reg::R0, Reg::R0), // NOP
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
                Some(guest_pc_end),
            );

            if DEBUG {
                for (index, inst_info) in self.jit_buf.instructions.iter().enumerate() {
                    let pc = index as u32 * pc_step_size + entry;
                    let (jit_addr, _) = jit_memory.get_jit_start_addr(pc).unwrap();

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

        self.jit_buf.clear_all();
        (addr, guest_pc_end)
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

        let (jit_entry, guest_pc_end) = {
            {
                let mut jit_memory = self.jit_memory.write().unwrap();

                {
                    let mut mem_handler = self.mem_handler.borrow_mut();
                    for addr in &mem_handler.invalidated_jit_addrs {
                        jit_memory.invalidate_block(*addr);
                    }
                    mem_handler.invalidated_jit_addrs.clear();
                }

                jit_memory.get_jit_start_addr(guest_pc)
            }
            .unwrap_or_else(|| self.emit_code_block(guest_pc, thumb))
        };

        self.mem_handler.borrow_mut().current_jit_block_range = (guest_pc, guest_pc_end);

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

        if DEBUG {
            debug_inst_info(
                self.cpu_type,
                RegReserve::gp(),
                self.thread_regs.borrow().deref(),
                self.guest_branch_out_pc,
                "breakout",
            );
        }
    }

    #[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
    unsafe extern "C" fn enter_jit(jit_entry: u32, host_sp_addr: u32, breakin_addr: u32) {
        asm!(
            "push {{r0-r12, lr}}",
            // Avoid R0-R3 here, compiler will try to optimize them for calling convention
            "mov r4, {jit_entry}",
            "mov r5, {host_sp_adr}",
            "mov r6, {breakin_addr}",
            "str sp, [r5]",
            "blx r6",
            "pop {{r0-r12, lr}}",
            jit_entry = in(reg) jit_entry,
            host_sp_adr = in(reg) host_sp_addr,
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

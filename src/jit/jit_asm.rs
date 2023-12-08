use crate::hle::cp15_context::Cp15Context;
use crate::hle::indirect_memory::indirect_memory_handler::IndirectMemoryHandler;
use crate::hle::registers::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg};
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, Mrs};
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_memory::JitMemory;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use crate::logging::debug_println;
use crate::memory::VmManager;
use crate::DEBUG;
use std::arch::asm;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::ptr;
use std::rc::Rc;

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
}

impl JitBuf {
    fn new() -> Self {
        JitBuf {
            instructions: Vec::new(),
            emit_opcodes: Vec::new(),
        }
    }
}

pub struct JitAsm {
    jit_memory: JitMemory,
    jit_addr_mapping: HashMap<u32, u32>,
    vmm: Rc<RefCell<VmManager>>,
    pub vm_mem_offset: u32,
    pub memory_offset: u32,
    pub cpu_type: CpuType,
    pub indirect_memory_handler: Rc<RefCell<IndirectMemoryHandler>>,
    pub thread_regs: Rc<RefCell<ThreadRegs>>,
    pub cp15_context: Rc<RefCell<Cp15Context>>,
    pub jit_buf: JitBuf,
    pub host_regs: Box<HostRegs>,
    pub breakin_addr: u32,
    pub breakout_addr: u32,
    pub breakout_skip_save_regs_addr: u32,
    pub restore_host_opcodes: [u32; 7],
    pub restore_guest_opcodes: [u32; 7],
}

impl JitAsm {
    pub fn new(
        vmm: Rc<RefCell<VmManager>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        cp15_context: Rc<RefCell<Cp15Context>>,
        cpu_type: CpuType,
    ) -> Self {
        let mut instance =
            {
                let jit_memory = JitMemory::new();
                let vm_begin_addr = vmm.borrow().vm_begin_addr() as u32;
                let base_offset = vmm.borrow().offset();

                println!(
                "{:?} JitAsm: Allocating jit memory at {:x} with vm at {:x} with base offset {:x}",
                cpu_type,
                jit_memory.memory.as_ptr() as u32,
                vm_begin_addr,
                base_offset
            );

                let host_regs = Box::new(HostRegs::default());

                let restore_host_opcodes = {
                    let mut opcodes = [0u32; 7];
                    // Save guest
                    opcodes[..4].copy_from_slice(&thread_regs.borrow().save_regs_opcodes);

                    // Restore host sp
                    let host_sp_addr = host_regs.get_sp_addr();
                    opcodes[4..6].copy_from_slice(&AluImm::mov32(Reg::LR, host_sp_addr));
                    opcodes[6] = LdrStrImm::ldr_al(Reg::SP, Reg::LR); // SP
                    opcodes
                };

                let restore_guest_opcodes = {
                    let mut opcodes = [0u32; 7];
                    // Restore guest
                    opcodes[0] = AluReg::mov_al(Reg::LR, Reg::SP);
                    opcodes[1..].copy_from_slice(&thread_regs.borrow().restore_regs_opcodes);
                    opcodes
                };

                JitAsm {
                    jit_memory,
                    jit_addr_mapping: HashMap::new(),
                    vmm: vmm.clone(),
                    vm_mem_offset: vm_begin_addr - base_offset,
                    memory_offset: base_offset,
                    cpu_type,
                    indirect_memory_handler: Rc::new(
                        RefCell::new(IndirectMemoryHandler::new(vmm, thread_regs.clone()))
                    ),
                    thread_regs,
                    cp15_context,
                    jit_buf: JitBuf::new(),
                    host_regs,
                    breakin_addr: 0,
                    breakout_addr: 0,
                    breakout_skip_save_regs_addr: 0,
                    restore_host_opcodes,
                    restore_guest_opcodes,
                }
            };

        {
            // Common function to enter guest (breakin)
            let breakin_jit_start = instance.jit_memory.open();

            // Save lr to return to this function
            instance
                .jit_memory
                .write_array(&AluImm::mov32(Reg::R0, instance.host_regs.get_lr_addr()));
            instance.jit_memory.write_array(&[
                LdrStrImm::str_al(Reg::LR, Reg::R0), // Save host lr
                Mrs::cpsr(Reg::LR, Cond::AL),
                LdrStrImm::str_offset_al(Reg::LR, Reg::R0, 4), // Save host cpsr
                AluReg::mov_al(Reg::LR, Reg::SP),              // Keep host sp in lr
                LdmStm::push_pre(reg_reserve!(Reg::R4), Reg::LR, Cond::AL), // Save actual entry
            ]);
            // Restore guest
            instance
                .jit_memory
                .write_array(&instance.thread_regs.borrow().restore_regs_opcodes);

            instance
                .jit_memory
                .write(LdmStm::pop_post(reg_reserve!(Reg::PC), Reg::LR, Cond::AL));

            instance.breakin_addr = instance.jit_memory.memory.as_ptr() as u32 + breakin_jit_start;

            // Common function to exit guest (breakout)
            instance.jit_memory.align_up();
            let breakout_jit_start = instance.jit_memory.ptr;

            // Save guest
            instance
                .jit_memory
                .write_array(&instance.thread_regs.borrow().save_regs_opcodes);

            // Some emits already save regs themselves, add skip addr
            let jit_skip_save_regs_start = instance.jit_memory.ptr;

            let host_sp_addr = instance.host_regs.get_sp_addr();

            instance
                .jit_memory
                .write_array(&AluImm::mov32(Reg::R0, host_sp_addr));
            instance.jit_memory.write_array(&[
                LdrStrImm::ldr_al(Reg::SP, Reg::R0),           // Restore host SP
                LdrStrImm::ldr_offset_al(Reg::LR, Reg::R0, 4), // Restore host LR
                Bx::bx(Reg::LR, Cond::AL),
            ]);

            let jit_end = instance.jit_memory.close();
            instance.jit_memory.flush_cache(breakin_jit_start, jit_end);

            instance.breakout_addr =
                instance.jit_memory.memory.as_ptr() as u32 + breakout_jit_start;
            instance.breakout_skip_save_regs_addr =
                instance.jit_memory.memory.as_ptr() as u32 + jit_skip_save_regs_start;
        }

        instance
    }

    fn emit_code_block(&mut self, entry: u32, thumb: bool) -> u32 {
        self.jit_memory.align_up();
        let jit_begin = self.jit_memory.open();

        let mut emulated_regs_count = HashMap::<Reg, u32>::new();

        let vmm = self.vmm.clone();
        let vmm = vmm.borrow();
        let vmmap = vmm.get_vm_mapping();

        self.jit_buf.instructions.clear();

        let (_, opcodes, _) = unsafe { vmmap[entry as usize..].align_to::<u32>() };

        for opcode in opcodes {
            let (op, func) = lookup_opcode(*opcode);
            debug_println!("Decoding {:x} {:?}", *opcode, op);
            let inst_info = func(*opcode, *op);

            if DEBUG {
                for reg in (inst_info.src_regs + inst_info.out_regs).get_emulated_regs() {
                    *emulated_regs_count.entry(reg).or_insert(0) += 1;
                }
            }

            self.jit_buf.instructions.push(inst_info);

            if inst_info.out_regs.is_reserved(Reg::PC) {
                todo!()
            }

            if op.is_branch() && inst_info.cond == Cond::AL {
                break;
            }
        }

        if DEBUG {
            debug_println!("Emulated regs {:?}", emulated_regs_count);
        }

        let jit_emits_start = self.jit_memory.memory.as_ptr() as u32 + self.jit_memory.ptr;
        self.jit_buf.emit_opcodes.clear();
        for i in 0..self.jit_buf.instructions.len() {
            let pc = i as u32 * 4 + entry;
            let jit_pc = self.jit_buf.emit_opcodes.len() as u32 * 4 + jit_emits_start;
            self.jit_addr_mapping.insert(pc, jit_pc);

            if DEBUG {
                self.jit_buf
                    .emit_opcodes
                    .push(AluReg::mov_al(Reg::R0, Reg::R0)); // NOP

                let inst_info = &self.jit_buf.instructions[i];
                debug_println!("Mapping {:#010x} to {:#010x} {:?}", pc, jit_pc, inst_info);
            }

            self.emit(i, pc);

            if DEBUG {
                let inst_info = &self.jit_buf.instructions[i];

                if !inst_info.op.is_branch() || inst_info.cond != Cond::AL {
                    self.jit_buf.emit_opcodes.extend(&[
                        AluReg::mov_al(Reg::R0, Reg::R0), // NOP
                        AluReg::mov_al(Reg::R0, Reg::R0), // NOP
                    ]);

                    self.emit_call_host_func(
                        |_| {},
                        &[Some(self as *const _ as u32), Some(pc)],
                        debug_after_exec_op as _,
                    );
                }
            }
        }
        // TODO statically analyze generated insts
        self.jit_memory.write_array(&self.jit_buf.emit_opcodes);

        let jit_end = self.jit_memory.close();
        self.jit_memory.flush_cache(jit_begin, jit_end);

        self.jit_memory.memory.as_ptr() as u32 + jit_begin
    }

    pub fn execute(&mut self) {
        let entry = self.thread_regs.borrow().pc;

        debug_println!("Execute {:x}", entry);

        let thumb = (entry & 1) == 1;
        if thumb {
            todo!()
        }
        let entry = entry & !1;

        let jit_entry = match self.jit_addr_mapping.get(&entry) {
            Some(jit_addr) => *jit_addr,
            None => self.emit_code_block(entry, thumb),
        };

        unsafe { JitAsm::enter_jit(jit_entry, self.host_regs.get_sp_addr(), self.breakin_addr) };

        if DEBUG {
            debug_inst_info(
                self.cpu_type,
                RegReserve::gp(),
                self.thread_regs.borrow().deref(),
                0,
            );
        }
    }

    #[inline(never)]
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

fn debug_inst_info(cpu_type: CpuType, regs_to_log: RegReserve, regs: &ThreadRegs, pc: u32) {
    let mut output = "Executed ".to_owned();
    for reg in reg_reserve!(Reg::SP, Reg::LR, Reg::PC, Reg::CPSR) + regs_to_log {
        let value = if reg != Reg::PC {
            *regs.get_reg_value(reg)
        } else {
            pc
        };
        output += &format!("{:?}: {:x}, ", reg, value);
    }
    println!("{:?} {}", cpu_type, output);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
unsafe extern "C" fn debug_after_exec_op(asm: *const JitAsm, pc: u32) {
    let inst_info = {
        let vmm = (*asm).vmm.borrow();
        let vmmap = vmm.get_vm_mapping();

        let (_, aligned, _) = vmmap[pc as usize..].align_to::<u32>();
        let opcode = aligned[0];
        let (op, func) = lookup_opcode(opcode);
        func(opcode, *op)
    };

    let regs = (*asm).thread_regs.borrow();
    debug_inst_info(
        (*asm).cpu_type,
        inst_info.src_regs + inst_info.out_regs,
        regs.deref(),
        pc,
    );
    println!("\t{:?}", inst_info);
}

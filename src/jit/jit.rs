use crate::hle::cp15_context::Cp15Context;
use crate::hle::thread_context::ThreadRegs;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg};
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, Msr};
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{reg_reserve, Reg};
use crate::jit::Cond;
use crate::logging::debug_println;
use crate::memory::VmManager;
use crate::mmap::Mmap;
use crate::utils::align_up;
use crate::DEBUG;
use std::arch::asm;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::{mem, ptr};

pub struct JitMemory {
    memory: Mmap,
    ptr: u32,
    is_open: bool,
}

impl JitMemory {
    fn new() -> Self {
        JitMemory {
            memory: Mmap::new("code", true, 16 * 1024 * 1024).unwrap(),
            ptr: 0,
            is_open: false,
        }
    }

    fn align_up(&mut self) {
        let current_addr = self.memory.as_ptr() as u32 + self.ptr;
        let aligned_addr = align_up(current_addr, 16);
        self.ptr += aligned_addr - current_addr;
    }

    pub fn write<T: Into<u32>>(&mut self, value: T) {
        debug_assert!(self.is_open);
        let (_, aligned, _) = unsafe { self.memory[self.ptr as usize..].align_to_mut::<T>() };
        aligned[0] = value;
        self.ptr += mem::size_of::<T>() as u32;
    }

    pub fn write_array<T: Into<u32>>(&mut self, value: &[T]) {
        debug_assert!(self.is_open);
        let (_, aligned_value, _) = unsafe { value.align_to::<u8>() };
        self.memory[self.ptr as usize..self.ptr as usize + aligned_value.len()]
            .copy_from_slice(aligned_value);
        self.ptr += (mem::size_of::<T>() * value.len()) as u32;
    }

    #[cfg(target_os = "linux")]
    fn open(&mut self) -> u32 {
        self.is_open = true;
        self.ptr
    }

    #[cfg(target_os = "linux")]
    fn close(&mut self) -> u32 {
        self.is_open = false;
        self.ptr
    }

    #[cfg(target_os = "linux")]
    fn flush_cache(&self, _: u32, _: u32) {}

    #[cfg(not(target_os = "linux"))]
    fn open(&mut self) -> u32 {
        let ret = unsafe { vitasdk_sys::sceKernelOpenVMDomain() };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't open vm domain {}", ret);
        }
        self.is_open = true;
        self.ptr
    }

    #[cfg(not(target_os = "linux"))]
    fn close(&mut self) -> u32 {
        let ret = unsafe { vitasdk_sys::sceKernelCloseVMDomain() };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't close vm domain {}", ret);
        }
        self.is_open = false;
        self.ptr
    }

    #[cfg(not(target_os = "linux"))]
    fn flush_cache(&self, begin: u32, end: u32) {
        let ret = unsafe {
            vitasdk_sys::sceKernelSyncVMDomain(
                self.memory.block_uid,
                (self.memory.as_ptr() as u32 + begin) as _,
                end - begin,
            )
        };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't sync vm domain {}", ret)
        }
    }
}

#[derive(Default)]
pub struct HostRegs {
    sp: u32,
    lr: u32,
    cprs: u32,
}

impl HostRegs {
    pub fn get_sp_addr(&self) -> u32 {
        ptr::addr_of!(self.sp) as _
    }

    pub fn get_lr_addr(&self) -> u32 {
        ptr::addr_of!(self.lr) as _
    }

    pub fn get_cpsr_addr(&self) -> u32 {
        ptr::addr_of!(self.cprs) as _
    }
}

pub struct JitAsm {
    jit_memory: JitMemory,
    jit_addr_mapping: HashMap<u32, u32>,
    vmm: Rc<RefCell<VmManager>>,
    pub vm_mem_offset: u32,
    pub memory_offset: u32,
    pub thread_regs: Rc<RefCell<ThreadRegs>>,
    pub cp15_context: Rc<RefCell<Cp15Context>>,
    pub opcode_buf: Vec<InstInfo>,
    pub jit_buf: Vec<u32>,
    pub host_regs: Box<HostRegs>,
    pub breakin_addr: u32,
    pub breakout_addr: u32,
    pub breakout_skip_save_regs_addr: u32,
    pub restore_host_opcodes: [u32; 11],
    pub restore_guest_opcodes: [u32; 7],
}

impl JitAsm {
    pub fn new(
        vmm: Rc<RefCell<VmManager>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        cp15_context: Rc<RefCell<Cp15Context>>,
    ) -> Self {
        let mut instance = {
            let jit_memory = JitMemory::new();
            let vm_begin_addr = vmm.borrow().vm_begin_addr() as u32;
            let base_offset = vmm.borrow().offset();

            println!(
                "JitAsm: Allocating jit memory at {:x} with vm at {:x} with base offset {:x}",
                jit_memory.memory.as_ptr() as u32,
                vm_begin_addr,
                base_offset
            );

            let host_regs = Box::new(HostRegs::default());

            let mut restore_host_opcodes = [0u32; 11];
            // Save guest
            restore_host_opcodes[..6].copy_from_slice(&thread_regs.borrow().save_regs_opcodes);

            // Restore host sp, cpsr
            let host_sp_addr = host_regs.get_sp_addr();
            restore_host_opcodes[6..8].copy_from_slice(&AluImm::mov32(Reg::LR, host_sp_addr));
            restore_host_opcodes[8] = LdrStrImm::ldr_al(Reg::SP, Reg::LR); // SP
            restore_host_opcodes[9] = LdrStrImm::ldr_offset_al(Reg::LR, Reg::LR, 8); // CPSR
            restore_host_opcodes[10] = Msr::cpsr(Reg::LR, Cond::AL);

            let mut restore_guest_opcodes = [0u32; 7];
            restore_guest_opcodes[0] = AluReg::mov_al(Reg::LR, Reg::SP);
            restore_guest_opcodes[1..].copy_from_slice(&thread_regs.borrow().restore_regs_opcodes);

            JitAsm {
                jit_memory,
                jit_addr_mapping: HashMap::new(),
                vmm: vmm.clone(),
                vm_mem_offset: vm_begin_addr - base_offset,
                memory_offset: base_offset,
                thread_regs,
                cp15_context,
                opcode_buf: Vec::new(),
                jit_buf: Vec::new(),
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
                LdrStrImm::str_al(Reg::LR, Reg::R0), // Save host LR
                AluReg::mov_al(Reg::LR, Reg::SP),    // Keep host sp in lr
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
                LdrStrImm::ldr_offset_al(Reg::R0, Reg::R0, 8), // Restore host CPSR
                Msr::cpsr(Reg::R0, Cond::AL),
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

    extern "C" fn debug_after_exec_op(&self, pc: u32) {
        let inst_info =
            {
                let vmm = self.vmm.borrow();
                let vmmap = vmm.get_vm_mapping();

                let (_, aligned, _) = unsafe { vmmap[pc as usize..].align_to::<u32>() };
                let opcode = aligned[0];
                let (op, func) = lookup_opcode(opcode);
                func(opcode, *op)
            };

        let regs = self.thread_regs.borrow();
        let mut output = "Executed ".to_owned();
        for reg in reg_reserve!(Reg::SP, Reg::LR, Reg::PC, Reg::CPSR)
            + inst_info.src_regs
            + inst_info.out_regs
        {
            let value =
                if reg != Reg::PC {
                    regs.get_reg_value(reg)
                } else {
                    pc
                };
            output += &format!("{:?}: {:x}, ", reg, value);
        }
        println!("{}\n\t{:?}", output, inst_info);
    }

    fn emit_code_block(&mut self, entry: u32, thumb: bool) -> u32 {
        self.jit_memory.align_up();
        let jit_begin = self.jit_memory.open();

        self.opcode_buf.clear();
        let mut emulated_regs_count = HashMap::<Reg, u32>::new();

        let vmm = self.vmm.clone();
        let vmm = vmm.borrow();
        let vmmap = vmm.get_vm_mapping();

        let (_, opcodes, _) = unsafe { vmmap[entry as usize..].align_to::<u32>() };

        for opcode in opcodes {
            let (op, func) = lookup_opcode(*opcode);
            debug_println!("Decoding {:?}", op);
            let inst_info = func(*opcode, *op);

            if DEBUG {
                for reg in (inst_info.src_regs + inst_info.out_regs).get_emulated_regs() {
                    *emulated_regs_count.entry(reg).or_insert(0) += 1;
                }
            }

            self.opcode_buf.push(inst_info);

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
        self.jit_buf.clear();
        for i in 0..self.opcode_buf.len() {
            let pc = i as u32 * 4 + entry;
            let jit_pc = self.jit_buf.len() as u32 * 4 + jit_emits_start;
            self.jit_addr_mapping.insert(pc, jit_pc);

            if DEBUG {
                self.jit_buf.push(AluReg::mov_al(Reg::R0, Reg::R0)); // NOP

                let inst_info = &self.opcode_buf[i];
                debug_println!("Mapping {:#010x} to {:#010x} {:?}", pc, jit_pc, inst_info);
            }

            self.emit(i, pc);

            if DEBUG {
                let debug_after_addr = JitAsm::debug_after_exec_op as *const () as u32;
                self.emit_call_host_func(
                    |_| {},
                    &[Some(self as *const _ as u32), Some(pc)],
                    debug_after_addr,
                );
            }
        }
        // TODO statically analyze generated insts
        self.jit_memory.write_array(&self.jit_buf);

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

        let host_sp_adr = self.host_regs.get_sp_addr();
        let host_cpsr_addr = self.host_regs.get_cpsr_addr();
        let breakin_addr = self.breakin_addr;

        unsafe {
            asm!(
                "push {{r0-r12, lr}}",
                // Avoid R0-R3 here, compiler will try to optimize them for calling convention
                "mov r4, {jit_entry}",
                "mov r5, {host_sp_adr}",
                "mov r6, {host_cpsr_addr}",
                "mov r7, {breakin_addr}",
                "mrs r8, cpsr",
                "str sp, [r5]",
                "str r8, [r6]",
                "blx r7",
                "pop {{r0-r12, lr}}",
                jit_entry = in(reg) jit_entry,
                host_sp_adr = in(reg) host_sp_adr,
                host_cpsr_addr = in(reg) host_cpsr_addr,
                breakin_addr = in(reg) breakin_addr,
            );
        }

        {
            let regs = self.thread_regs.borrow();
            debug_println!(
                "Exiting gp: {:?}, sp: {:x}, lr: {:x}, pc: {:x}, cpsr: {:x}",
                regs.gp_regs,
                regs.sp,
                regs.lr,
                regs.pc,
                regs.cpsr,
            );
        }
    }
}

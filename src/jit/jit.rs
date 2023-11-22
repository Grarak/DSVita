use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::thread_context::ThreadRegs;
use crate::jit::{InstInfo, Op};
use crate::logging::debug_println;
use crate::memory::VmManager;
use crate::mmap::Mmap;
use std::arch::asm;
use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

pub struct JitMemory {
    memory: Mmap,
    ptr: usize,
}

impl JitMemory {
    fn new() -> Self {
        JitMemory {
            memory: Mmap::new("code", true, 16 * 1024 * 1024).unwrap(),
            ptr: 0,
        }
    }

    fn begin_addr(&self) -> *const u8 {
        self.memory.as_ptr()
    }

    pub fn write<T: Into<u32>>(&mut self, value: T) {
        let (_, aligned, _) = unsafe { self.memory[self.ptr..].align_to_mut::<T>() };
        aligned[0] = value;
        self.ptr += mem::size_of::<T>();
    }

    pub fn write_array<T: Into<u32>>(&mut self, value: &[T]) {
        let (_, aligned_value, _) = unsafe { value.align_to::<u8>() };
        self.memory[self.ptr..self.ptr + aligned_value.len()].copy_from_slice(aligned_value);
        self.ptr += mem::size_of::<T>() * value.len();
    }
}

pub struct JitAsm {
    pub jit_memory: JitMemory,
    vmm: Rc<RefCell<VmManager>>,
    pub vm_mem_offset: u32,
    pub memory_offset: u32,
    pub thread_regs: Rc<RefCell<ThreadRegs>>,
    pub opcode_buf: Vec<(u32, Op, InstInfo)>,
}

impl JitAsm {
    pub fn new(vmm: Rc<RefCell<VmManager>>, thread_regs: Rc<RefCell<ThreadRegs>>) -> Self {
        let jit_memory = JitMemory::new();
        let vm_start = vmm.borrow().vm.as_ptr() as u32;
        let base_offset = vmm.borrow().offset();

        println!(
            "JitAsm: Allocating jit memory at {:x} with base offset {:x}",
            jit_memory.memory.as_ptr() as u32,
            base_offset
        );

        JitAsm {
            jit_memory,
            vmm: vmm.clone(),
            vm_mem_offset: vm_start - base_offset,
            memory_offset: base_offset,
            thread_regs,
            opcode_buf: Vec::new(),
        }
    }

    pub fn execute(&mut self, entry: u32) {
        debug_println!("execute {:x}", entry);

        let vmm = self.vmm.clone();
        let vmm = vmm.borrow();

        let (_, opcodes, _) =
            unsafe { vmm.vm[(entry - self.memory_offset) as usize..].align_to::<u32>() };

        self.jit_memory
            .write_array(&self.thread_regs.borrow().emit_restore_regs());

        self.opcode_buf.clear();
        for opcode in opcodes {
            let (op, func) = lookup_opcode(*opcode);
            let inst_info = func(*opcode);
            debug_println!("{:?} {:?}", op, inst_info);

            self.opcode_buf.push((*opcode, *op, inst_info));

            if op.is_branch() {
                break;
            }
        }

        for i in 0..self.opcode_buf.len() {
            self.emit(i, i as u32 * 4 + entry);
        }

        let jit_entry = self.jit_memory.begin_addr() as u32;
        unsafe {
            asm!(
                "push {{r0-r12, lr}}",
                "mov r0, {jit_entry}",
                "mov lr, pc",
                "bx r0",
                "pop {{r0-r12, lr}}",
                jit_entry = in(reg) jit_entry,
            );
        }
    }
}

use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::memory::indirect_memory::MemoryAmount;
use crate::hle::memory::io_ports::IoPorts;
use crate::hle::memory::memory::Memory;
use crate::hle::memory::regions;
use crate::hle::registers::ThreadRegs;
use crate::hle::spu_context::SpuContext;
use crate::hle::CpuType;
use crate::host_memory::VmManager;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::inst_info::{Operand, ShiftValue};
use crate::jit::reg::Reg;
use crate::jit::{Op, ShiftType};
use crate::logging::debug_println;
use crate::utils;
use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub struct IndirectMemHandler {
    cpu_type: CpuType,
    memory: Arc<Mutex<Memory>>,
    pub vmm: *mut VmManager,
    pub thread_regs: Rc<RefCell<ThreadRegs>>,
    io_ports: IoPorts,
    pub invalidated_jit_addrs: HashSet<u32>,
}

pub enum WriteBack {
    Byte(u8),
    Half(u16),
    Word(u32),
    None,
}

impl IndirectMemHandler {
    pub fn new(
        cpu_type: CpuType,
        memory: Arc<Mutex<Memory>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        gpu_context: Rc<RefCell<GpuContext>>,
        spu_context: Rc<RefCell<SpuContext>>,
    ) -> Self {
        let vmm = &mut memory.lock().unwrap().vmm as *mut _;
        IndirectMemHandler {
            cpu_type,
            memory: memory.clone(),
            vmm,
            thread_regs: thread_regs.clone(),
            io_ports: IoPorts::new(memory.clone(), thread_regs, gpu_context, spu_context),
            invalidated_jit_addrs: HashSet::new(),
        }
    }

    fn handle_request(&mut self, opcode: u32, pc: u32, write: bool) {
        debug_println!(
            "{:?} indirect memory {} {:x}",
            self.cpu_type,
            if write { "write" } else { "read" },
            pc
        );

        let inst_info = {
            let (op, func) = lookup_opcode(opcode);
            func(opcode, *op)
        };

        let pre = match inst_info.op {
            Op::LdrOfip | Op::StrOfip | Op::StrhOfip | Op::StrPrim => true,
            _ => todo!(),
        };

        let write_back = match inst_info.op {
            Op::LdrOfip | Op::StrOfip | Op::StrhOfip => false,
            Op::StrPrim => true,
            _ => todo!(),
        };

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op1 = operands[1].as_reg_no_shift().unwrap();

        let get_reg_value =
            |regs: &ThreadRegs, reg: Reg| match reg {
                Reg::PC => pc + 8,
                _ => *regs.get_reg_value(reg),
            };
        let set_reg_value = |regs: &mut ThreadRegs, reg: Reg, value: u32| match reg {
            Reg::PC => todo!(),
            _ => *regs.get_reg_value_mut(reg) = value,
        };

        let new_base = {
            let mut op1_value = get_reg_value(self.thread_regs.borrow().deref(), *op1);

            if operands.len() == 3 {
                let op2_value = match &operands[2] {
                    Operand::Reg { reg, shift } => {
                        let mut op2_value = get_reg_value(self.thread_regs.borrow().deref(), *reg);

                        if let Some(shift) = shift {
                            let (shift_type, shift) = (*shift).into();
                            let shift_value = match shift {
                                ShiftValue::Reg(reg) => {
                                    get_reg_value(self.thread_regs.borrow().deref(), reg)
                                }
                                ShiftValue::Imm(imm) => imm as u32,
                            };

                            match shift_type {
                                ShiftType::LSL => {
                                    op2_value = unsafe { op2_value.unchecked_shl(shift_value) }
                                }
                                ShiftType::LSR => {
                                    op2_value = unsafe { op2_value.unchecked_shr(shift_value) }
                                }
                                ShiftType::ASR => {
                                    op2_value =
                                        unsafe { (op2_value as i32).unchecked_shr(shift_value) }
                                            as u32
                                }
                                ShiftType::ROR => {
                                    op2_value = unsafe {
                                        op2_value.unchecked_shr(shift_value)
                                            | op2_value.unchecked_shl(32 - shift_value)
                                    };
                                }
                            }
                        }

                        op2_value
                    }
                    Operand::Imm(imm) => *imm,
                    _ => panic!(),
                };

                op1_value = unsafe { op1_value.unchecked_add(op2_value) };
            }

            op1_value
        };

        let base_addr = if pre {
            if write_back {
                set_reg_value(self.thread_regs.borrow_mut().deref_mut(), *op1, new_base);
            }
            new_base
        } else {
            get_reg_value(self.thread_regs.borrow().deref(), *op1)
        };

        let memory_amount = MemoryAmount::from(inst_info.op);
        match memory_amount {
            MemoryAmount::BYTE => {
                if write {
                    let value = get_reg_value(self.thread_regs.borrow().deref(), *op0);
                    self.write(base_addr, value as u8);
                } else {
                    let value = self.read::<u8>(base_addr);
                    set_reg_value(
                        self.thread_regs.borrow_mut().deref_mut(),
                        *op0,
                        value as u32,
                    );
                }
            }
            MemoryAmount::HALF => {
                if write {
                    let value = get_reg_value(self.thread_regs.borrow().deref(), *op0);
                    self.write(base_addr, value as u16);
                } else {
                    let value = self.read::<u16>(base_addr);
                    set_reg_value(
                        self.thread_regs.borrow_mut().deref_mut(),
                        *op0,
                        value as u32,
                    );
                }
            }
            MemoryAmount::WORD => {
                if write {
                    let value = get_reg_value(self.thread_regs.borrow().deref(), *op0);
                    self.write(base_addr, value);
                } else {
                    let value = self.read(base_addr);
                    set_reg_value(self.thread_regs.borrow_mut().deref_mut(), *op0, value);
                }
            }
            MemoryAmount::DOUBLE => {
                if write {
                    let value = get_reg_value(self.thread_regs.borrow().deref(), *op0);
                    let value1 =
                        get_reg_value(self.thread_regs.borrow().deref(), Reg::from(*op0 as u8 + 1));
                    self.write(base_addr, value);
                    self.write(base_addr + 4, value1);
                } else {
                    let value = self.read(base_addr);
                    let value1 = self.read(base_addr);
                    set_reg_value(self.thread_regs.borrow_mut().deref_mut(), *op0, value);
                    set_reg_value(
                        self.thread_regs.borrow_mut().deref_mut(),
                        Reg::from(*op0 as u8 + 1),
                        value1,
                    );
                }
            }
        }

        if !pre && write_back {
            set_reg_value(self.thread_regs.borrow_mut().deref_mut(), *op1, new_base);
        }
    }

    pub fn read<T: Copy + Into<u32>>(&self, addr: u32) -> T {
        match self.cpu_type {
            CpuType::ARM7 => self.read_arm7(addr),
            CpuType::ARM9 => todo!(),
        }
    }

    pub fn write<T: Copy + Into<u32>>(&mut self, addr: u32, value: T) {
        match self.cpu_type {
            CpuType::ARM7 => self.write_arm7(addr, value),
            CpuType::ARM9 => self.write_arm9(addr, value),
        }
    }

    fn read_arm7<T: Copy + Into<u32>>(&self, addr: u32) -> T {
        let base = addr & 0xFF000000;
        let offset = addr - base;

        match base {
            regions::MAIN_MEMORY_OFFSET => self.read_raw(addr),
            regions::SHARED_WRAM_OFFSET => self.memory.lock().unwrap().read_wram_arm7(offset),
            regions::ARM7_IO_PORTS_OFFSET => self.io_ports.read_arm7(offset),
            _ => todo!("unimplemented read addr {:x}", addr),
        }
    }

    fn write_arm7<T: Copy + Into<u32>>(&mut self, addr: u32, value: T) {
        let base = addr & 0xFF000000;
        let offset = addr - base;

        let write_to_mem = match base {
            regions::MAIN_MEMORY_OFFSET => true,
            regions::SHARED_WRAM_OFFSET => {
                self.memory.lock().unwrap().write_wram_arm7(offset, value);
                self.invalidated_jit_addrs.insert(addr);
                false
            }
            regions::ARM7_IO_PORTS_OFFSET => {
                self.io_ports.write_arm7(offset, value);
                false
            }
            _ => todo!("unimplemented write addr {:x}", addr),
        };

        if write_to_mem {
            self.write_raw(addr, value)
        }
    }

    fn write_arm9<T: Copy + Into<u32>>(&self, addr: u32, value: T) {
        let base = addr & 0xFF000000;
        let offset = addr - base;

        let readjusted_value = match base {
            regions::SHARED_WRAM_OFFSET => {
                self.memory.lock().unwrap().write_wram_arm9(offset, value);
                None
            }
            regions::ARM9_IO_PORTS_OFFSET => Some(self.io_ports.write_arm9(offset, value.clone())),
            regions::STANDARD_PALETTES_OFFSET => todo!(),
            regions::VRAM_ENGINE_A_BG_OFFSET => todo!(),
            regions::VRAM_ENGINE_B_BG_OFFSET => todo!(),
            regions::VRAM_ENGINE_A_OBJ_OFFSET => todo!(),
            regions::VRAM_ENGINE_B_OBJ_OFFSET => todo!(),
            regions::VRAM_LCDC_ALLOCATED_OFFSET => todo!(),
            _ => Some(WriteBack::None),
        };

        if let Some(readjusted_value) = readjusted_value {
            match readjusted_value {
                WriteBack::Byte(v) => self.write_raw(addr, v),
                WriteBack::Half(v) => self.write_raw(addr, v),
                WriteBack::Word(v) => self.write_raw(addr, v),
                WriteBack::None => self.write_raw(addr, value),
            }
        }
    }

    fn read_raw<T: Copy + Into<u32>>(&self, addr: u32) -> T {
        let vmmap = unsafe { (*self.vmm).get_vm_mapping() };
        utils::read_from_mem(&vmmap, addr)
    }

    fn write_raw<T: Into<u32>>(&self, addr: u32, value: T) {
        let mut vmmap = unsafe { (*self.vmm).get_vm_mapping() };
        utils::write_to_mem(&mut vmmap, addr, value)
    }
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_read(handler: *mut IndirectMemHandler, opcode: u32, pc: u32) {
    (*handler).handle_request(opcode, pc, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_write(
    handler: *mut IndirectMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request(opcode, pc, true);
}

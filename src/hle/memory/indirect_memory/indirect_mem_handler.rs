use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::indirect_memory::{Convert, MemoryAmount};
use crate::hle::memory::io_ports::IoPorts;
use crate::hle::memory::memory::Memory;
use crate::hle::memory::regions;
use crate::hle::registers::ThreadRegs;
use crate::hle::spu_context::SpuContext;
use crate::hle::CpuType;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::{InstInfo, Operand, ShiftValue};
use crate::jit::reg::Reg;
use crate::jit::{Op, ShiftType};
use crate::logging::debug_println;
use std::cell::RefCell;
use std::collections::HashSet;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct IndirectMemHandler {
    pub cpu_type: CpuType,
    memory: Arc<RwLock<Memory>>,
    pub thread_regs: Rc<RefCell<ThreadRegs>>,
    io_ports: IoPorts,
    pub invalidated_jit_addrs: HashSet<u32>,
}

impl IndirectMemHandler {
    pub fn new(
        cpu_type: CpuType,
        memory: Arc<RwLock<Memory>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        gpu_context: Rc<RefCell<GpuContext>>,
        spu_context: Rc<RefCell<SpuContext>>,
    ) -> Self {
        IndirectMemHandler {
            cpu_type,
            memory: memory.clone(),
            thread_regs: thread_regs.clone(),
            io_ports: IoPorts::new(
                memory.clone(),
                ipc_handler,
                thread_regs,
                gpu_context,
                spu_context,
            ),
            invalidated_jit_addrs: HashSet::new(),
        }
    }

    fn handle_request(&mut self, opcode: u32, pc: u32, thumb: bool, write: bool) {
        let inst_info = {
            if thumb {
                let (op, func) = lookup_thumb_opcode(opcode as u16);
                InstInfo::from(&func(opcode as u16, *op))
            } else {
                let (op, func) = lookup_opcode(opcode);
                func(opcode, *op)
            }
        };

        let pre = match inst_info.op {
            Op::LdrOfip
            | Op::LdrbOfrplr
            | Op::StrOfip
            | Op::StrhOfip
            | Op::StrPrim
            | Op::LdrPcT => true,
            Op::LdrPtip => false,
            _ => todo!("{:?}", inst_info),
        };

        let write_back = match inst_info.op {
            Op::LdrOfip | Op::LdrbOfrplr | Op::StrOfip | Op::StrhOfip | Op::LdrPcT => false,
            Op::LdrPtip | Op::StrPrim => true,
            _ => todo!("{:?}", inst_info),
        };

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op1 = operands[1].as_reg_no_shift().unwrap();

        let get_reg_value = |regs: &ThreadRegs, reg: Reg| match reg {
            Reg::PC => pc + 4 + 4 * !thumb as u32,
            _ => *regs.get_reg_value(reg),
        };
        let set_reg_value = |regs: &mut ThreadRegs, reg: Reg, value: u32| {
            *regs.get_reg_value_mut(reg) = value;
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

        let mut base_addr = if pre {
            if write_back {
                set_reg_value(self.thread_regs.borrow_mut().deref_mut(), *op1, new_base);
            }
            new_base
        } else {
            get_reg_value(self.thread_regs.borrow().deref(), *op1)
        };

        if inst_info.op == Op::LdrPcT {
            base_addr &= !0x3;
        }

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

                    if self.cpu_type == CpuType::ARM7 && base_addr & 1 != 0 {
                        todo!()
                    }

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

                    if base_addr & 3 != 0 {
                        todo!("{:?} {:x}", self.cpu_type, pc);
                    }

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

    pub fn read<T: Convert>(&self, addr: u32) -> T {
        let ret: T = match self.cpu_type {
            CpuType::ARM7 => self.read_arm7(addr),
            CpuType::ARM9 => self.read_raw(addr),
        };

        debug_println!(
            "{:?} indirect memory read at {:x} with value {:x}",
            self.cpu_type,
            addr,
            ret.into()
        );

        ret
    }

    pub fn read_slice<T: Convert>(&self, addr: u32, slice: &mut [T]) {
        let addr_end = addr + (slice.len() * mem::size_of::<T>()) as u32;

        let addr_base = addr & 0xFF000000;
        let addr_end_base = addr_end & 0xFF000000;
        debug_assert_eq!(addr_base, addr_end_base);

        match self.cpu_type {
            CpuType::ARM7 => self.read_slice_arm7(addr, slice),
            CpuType::ARM9 => self.read_slice_raw(addr, slice),
        }
    }

    pub fn write<T: Convert>(&mut self, addr: u32, value: T) {
        debug_println!(
            "{:?} indirect memory write at {:x} with value {:x}",
            self.cpu_type,
            addr,
            value.into(),
        );

        match self.cpu_type {
            CpuType::ARM7 => self.write_arm7(addr, value),
            CpuType::ARM9 => self.write_arm9(addr, value),
        }
    }

    fn read_arm7<T: Convert>(&self, addr: u32) -> T {
        let base = addr & 0xFF000000;
        let offset = addr - base;

        match base {
            regions::MAIN_MEMORY_OFFSET => self.read_raw(addr),
            regions::SHARED_WRAM_OFFSET => self.memory.read().unwrap().read_wram_arm7(offset),
            regions::ARM7_IO_PORTS_OFFSET => self.io_ports.read_arm7(offset),
            _ => todo!("unimplemented read addr {:x}", addr),
        }
    }

    fn read_slice_arm7<T: Convert>(&self, addr: u32, slice: &mut [T]) {
        let base = addr & 0xFF000000;
        let offset = addr - base;

        match base {
            regions::MAIN_MEMORY_OFFSET => self.read_slice_raw(addr, slice),
            regions::SHARED_WRAM_OFFSET => self
                .memory
                .read()
                .unwrap()
                .read_wram_slice_arm7(offset, slice),
            _ => todo!("unimplemented read slice addr {:x}", addr),
        }
    }

    fn write_arm7<T: Convert>(&mut self, addr: u32, value: T) {
        let base = addr & 0xFF000000;
        let offset = addr - base;

        let write_to_mem = match base {
            regions::MAIN_MEMORY_OFFSET => true,
            regions::SHARED_WRAM_OFFSET => {
                self.memory.write().unwrap().write_wram_arm7(offset, value);
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

    fn write_arm9<T: Convert>(&self, addr: u32, value: T) {
        let base = addr & 0xFF000000;
        let offset = addr - base;

        let readjusted_value = match base {
            regions::SHARED_WRAM_OFFSET => {
                self.memory.write().unwrap().write_wram_arm9(offset, value);
                None
            }
            regions::ARM9_IO_PORTS_OFFSET => Some(self.io_ports.write_arm9(offset, value)),
            regions::STANDARD_PALETTES_OFFSET => todo!(),
            regions::VRAM_ENGINE_A_BG_OFFSET => todo!(),
            regions::VRAM_ENGINE_B_BG_OFFSET => todo!(),
            regions::VRAM_ENGINE_A_OBJ_OFFSET => todo!(),
            regions::VRAM_ENGINE_B_OBJ_OFFSET => todo!(),
            regions::VRAM_LCDC_ALLOCATED_OFFSET => todo!(),
            _ => Some(value),
        };

        if let Some(readjusted_value) = readjusted_value {
            self.write_raw(addr, readjusted_value)
        }
    }

    fn read_raw<T: Convert>(&self, addr: u32) -> T {
        self.memory.read().unwrap().read_raw(addr)
    }

    fn read_slice_raw<T: Convert>(&self, addr: u32, slice: &mut [T]) {
        self.memory.read().unwrap().read_raw_slice(addr, slice);
    }

    fn write_raw<T: Convert>(&self, addr: u32, value: T) {
        self.memory.write().unwrap().write_raw(addr, value)
    }
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_read(handler: *mut IndirectMemHandler, opcode: u32, pc: u32) {
    (*handler).handle_request(opcode, pc, false, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_write(
    handler: *mut IndirectMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request(opcode, pc, false, true);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_read_thumb(
    handler: *mut IndirectMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request(opcode, pc, true, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_write_thumb(
    handler: *mut IndirectMemHandler,
    opcode: u32,
    pc: u32,
) {
    (*handler).handle_request(opcode, pc, true, true);
}

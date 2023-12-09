use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::memory::indirect_memory::MemoryAmount;
use crate::hle::memory::io_ports::IoPorts;
use crate::hle::memory::memory::Memory;
use crate::hle::memory::regions;
use crate::hle::registers::ThreadRegs;
use crate::hle::CpuType;
use crate::host_memory::{VMmap, VmManager};
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::inst_info::{InstInfo, Operand, ShiftValue};
use crate::jit::reg::Reg;
use crate::jit::{Op, ShiftType};
use crate::logging::debug_println;
use std::cell::RefCell;
use std::rc::Rc;

pub struct IndirectMemHandler {
    cpu_type: CpuType,
    pub vmm: Rc<RefCell<VmManager>>,
    pub thread_regs: Rc<RefCell<ThreadRegs>>,
    io_ports: IoPorts,
}

impl IndirectMemHandler {
    pub fn new(
        cpu_type: CpuType,
        memory: Rc<RefCell<Memory>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        gpu_context: Rc<RefCell<GpuContext>>,
    ) -> Self {
        let vmm = memory.borrow().vmm.clone();
        IndirectMemHandler {
            cpu_type,
            vmm,
            thread_regs: thread_regs.clone(),
            io_ports: IoPorts::new(memory, thread_regs, gpu_context),
        }
    }

    pub fn get_inst_info(vmmap: &VMmap, addr: u32) -> InstInfo {
        let (_, aligned, _) = unsafe { vmmap[addr as usize..].align_to::<u32>() };
        let opcode = aligned[0];
        let (op, func) = lookup_opcode(opcode);
        func(opcode, *op)
    }

    fn handle_request(&self, pc: u32, write: bool) {
        debug_println!(
            "indirect memory {} {:x}",
            if write { "write" } else { "read" },
            pc
        );

        let inst_info = {
            let vmm = self.vmm.borrow();
            IndirectMemHandler::get_inst_info(&vmm.get_vm_mapping(), pc)
        };

        let pre =
            match inst_info.op {
                Op::LdrOfip | Op::StrOfip | Op::StrhOfip => true,
                _ => todo!(),
            };

        let add_to_base =
            match inst_info.op {
                Op::LdrOfip | Op::StrOfip | Op::StrhOfip => true,
                _ => todo!(),
            };

        let write_back =
            match inst_info.op {
                Op::LdrOfip | Op::StrOfip | Op::StrhOfip => false,
                _ => todo!(),
            };

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op1 = operands[1].as_reg_no_shift().unwrap();

        let get_reg_value = |reg: Reg| match reg {
            Reg::PC => pc + 8,
            _ => *self.thread_regs.borrow().get_reg_value(reg),
        };
        let set_reg_value = |reg: Reg, value: u32| match reg {
            Reg::PC => todo!(),
            _ => *self.thread_regs.borrow_mut().get_reg_value_mut(reg) = value,
        };

        let new_base =
            {
                let mut op1_value = get_reg_value(*op1);

                if operands.len() == 3 {
                    let op2_value = match &operands[2] {
                        Operand::Reg { reg, shift } => {
                            let mut op2_value = get_reg_value(*reg);

                            if let Some(shift) = shift {
                                let (shift_type, shift) = (*shift).into();
                                let shift_value = match shift {
                                    ShiftValue::Reg(reg) => get_reg_value(reg),
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

                    if add_to_base {
                        op1_value += op2_value;
                    } else {
                        op1_value -= op2_value;
                    }
                }

                op1_value
            };

        let base_addr =
            if pre {
                if write_back {
                    set_reg_value(*op1, new_base);
                }
                new_base
            } else {
                get_reg_value(*op1)
            };

        let memory_amount = MemoryAmount::from(inst_info.op);
        match memory_amount {
            MemoryAmount::BYTE => {
                if write {
                    let value = get_reg_value(*op0);
                    self.write(base_addr, value as u8);
                } else {
                    let value = self.read::<u8>(base_addr);
                    set_reg_value(*op0, value as u32);
                }
            }
            MemoryAmount::HALF => {
                if write {
                    let value = get_reg_value(*op0);
                    self.write(base_addr, value as u16);
                } else {
                    let value = self.read::<u16>(base_addr);
                    set_reg_value(*op0, value as u32);
                }
            }
            MemoryAmount::WORD => {
                if write {
                    let value = get_reg_value(*op0);
                    self.write(base_addr, value);
                } else {
                    let value = self.read(base_addr);
                    set_reg_value(*op0, value);
                }
            }
            MemoryAmount::DOUBLE => {
                if write {
                    let value = get_reg_value(*op0);
                    let value1 = get_reg_value(Reg::from(*op0 as u8 + 1));
                    self.write(base_addr, value);
                    self.write(base_addr + 4, value1);
                } else {
                    let value = self.read(base_addr);
                    let value1 = self.read(base_addr);
                    set_reg_value(*op0, value);
                    set_reg_value(Reg::from(*op0 as u8 + 1), value1);
                }
            }
        }

        if !pre && write_back {
            set_reg_value(*op1, new_base);
        }
    }

    pub fn read<T: Into<u32> + Clone>(&self, addr: u32) -> T {
        match self.cpu_type {
            CpuType::ARM7 => self.read_arm7(addr),
            CpuType::ARM9 => self.read_arm9(addr),
        }
    }

    pub fn write<T: Clone + Into<u32>>(&self, addr: u32, value: T) {
        debug_println!(
            "{:?} Writing to {:x} with value {:x}",
            self.cpu_type,
            addr,
            value.clone().into()
        );

        match self.cpu_type {
            CpuType::ARM7 => self.write_arm7(addr, value),
            CpuType::ARM9 => self.write_arm9(addr, value),
        }
    }

    fn read_arm7<T: Into<u32>>(&self, addr: u32) -> T {
        todo!()
    }

    fn write_arm7<T: Into<u32>>(&self, addr: u32, value: T) {
        todo!()
    }

    fn read_arm9<T: Into<u32>>(&self, addr: u32) -> T {
        todo!()
    }

    fn write_arm9<T: Clone + Into<u32>>(&self, addr: u32, value: T) {
        let vmm = self.vmm.borrow();
        let mut vmmap = vmm.get_vm_mapping();
        let (_, aligned, _) = unsafe { vmmap[addr as usize..].align_to_mut::<T>() };
        aligned[0] = value.clone();

        let base = addr & 0xFF000000;
        let offset = addr - base;
        match base {
            regions::SHARED_WRAM_OFFSET => {
                todo!()
            }
            regions::ARM9_IO_PORTS_OFFSET => self.io_ports.write_arm9(offset, value),
            regions::STANDARD_PALETTES_OFFSET => {
                todo!()
            }
            regions::VRAM_ENGINE_A_BG_OFFSET => {
                todo!()
            }
            regions::VRAM_ENGINE_B_BG_OFFSET => {
                todo!()
            }
            regions::VRAM_ENGINE_A_OBJ_OFFSET => {
                todo!()
            }
            regions::VRAM_ENGINE_B_OBJ_OFFSET => {
                todo!()
            }
            regions::VRAM_LCDC_ALLOCATED_OFFSET => {
                todo!()
            }
            _ => {}
        }
    }
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_read(handler: *const IndirectMemHandler, pc: u32) {
    (*handler).handle_request(pc, false);
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn indirect_mem_write(handler: *const IndirectMemHandler, pc: u32) {
    (*handler).handle_request(pc, true);
}

use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::indirect_memory::Convert;
use crate::hle::memory::memory::Memory;
use crate::hle::memory::regions;
use crate::hle::registers::ThreadRegs;
use crate::hle::spu_context::SpuContext;
use crate::hle::CpuType;
use crate::utils;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct IoPorts {
    memory: Arc<RwLock<Memory>>,
    ipc_handler: Arc<RwLock<IpcHandler>>,
    thread_regs: Rc<RefCell<ThreadRegs>>,
    gpu_context: Rc<RefCell<GpuContext>>,
    spu_context: Rc<RefCell<SpuContext>>,
}

impl IoPorts {
    pub fn new(
        memory: Arc<RwLock<Memory>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
        thread_regs: Rc<RefCell<ThreadRegs>>,
        gpu_context: Rc<RefCell<GpuContext>>,
        spu_context: Rc<RefCell<SpuContext>>,
    ) -> Self {
        IoPorts {
            memory,
            ipc_handler,
            thread_regs,
            gpu_context,
            spu_context,
        }
    }

    pub fn read_arm7<T: Convert>(&self, addr_offset: u32) -> T {
        T::from(match addr_offset {
            0x180 => self.ipc_handler.read().unwrap().get_sync_reg(CpuType::ARM7) as u32,
            _ => todo!("unimplemented io port read {:x}", addr_offset),
        })
    }

    fn write_common<T: Convert>(&self, cpu_type: CpuType, addr_offset: u32, value: T) -> T {
        let value = value.into();
        T::from(match addr_offset {
            0x208 => self.thread_regs.borrow_mut().set_ime(value as u8) as u32,
            0x300 => self.thread_regs.borrow_mut().set_post_flg(value as u8) as u32,
            _ => todo!("{:?} unimplemented io port {:x}", cpu_type, addr_offset),
        })
    }

    pub fn write_arm7<T: Convert>(&self, addr_offset: u32, value: T) -> T {
        match addr_offset {
            0x180 => {
                let (ret, arm9_ipc_sync_value) = {
                    let mut ipc_handler = self.ipc_handler.write().unwrap();
                    (
                        ipc_handler.set_sync_reg(CpuType::ARM7, value.into() as u16),
                        ipc_handler.get_sync_reg(CpuType::ARM9),
                    )
                };

                // Write to ARM9 mem
                let mut mem = self.memory.write().unwrap();
                let mut vmmap = mem.vmm.get_vm_mapping_mut();
                utils::write_to_mem(
                    &mut vmmap,
                    regions::ARM9_IO_PORTS_OFFSET | addr_offset,
                    arm9_ipc_sync_value,
                );

                T::from(ret as u32)
            }
            0x504 => T::from(
                self.spu_context
                    .borrow_mut()
                    .set_sound_bias(value.into() as u16) as u32,
            ),
            _ => self.write_common(CpuType::ARM7, addr_offset, value),
        }
    }

    pub fn write_arm9<T: Convert>(&self, addr_offset: u32, value: T) -> T {
        match addr_offset {
            0x180 => T::from(
                self.ipc_handler
                    .write()
                    .unwrap()
                    .set_sync_reg(CpuType::ARM9, value.into() as u16) as u32,
            ),
            0x247 => T::from(
                self.memory
                    .write()
                    .unwrap()
                    .set_wram_cnt(value.into() as u8) as u32,
            ),
            0x304 => T::from(
                self.gpu_context
                    .borrow_mut()
                    .set_pow_cnt1(value.into() as u16) as u32,
            ),
            _ => self.write_common(CpuType::ARM9, addr_offset, value),
        }
    }
}

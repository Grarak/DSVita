use crate::hle::cpu_regs::CpuRegs;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::dma::Dma;
use crate::hle::memory::io_ports::IoPorts;
use crate::hle::memory::memory::Memory;
use crate::hle::memory::{regions, Convert};
use crate::hle::spu_context::SpuContext;
use crate::hle::CpuType;
use crate::logging::debug_println;
use std::cell::RefCell;
use std::collections::HashSet;
use std::mem;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct MemHandler {
    pub cpu_type: CpuType,
    memory: Arc<RwLock<Memory>>,
    pub io_ports: IoPorts,
    pub invalidated_jit_addrs: HashSet<u32>,
    pub current_jit_block_range: (u32, u32),
}

impl MemHandler {
    pub fn new(
        cpu_type: CpuType,
        memory: Arc<RwLock<Memory>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
        cpu_regs: Rc<RefCell<CpuRegs>>,
        gpu_context: Rc<RefCell<GpuContext>>,
        spu_context: Rc<RefCell<SpuContext>>,
        dma: Rc<RefCell<Dma>>,
    ) -> Self {
        MemHandler {
            cpu_type,
            memory: memory.clone(),
            io_ports: IoPorts::new(
                cpu_type,
                memory.clone(),
                ipc_handler,
                cpu_regs,
                gpu_context,
                spu_context,
                dma,
            ),
            invalidated_jit_addrs: HashSet::new(),
            current_jit_block_range: (0, 0),
        }
    }

    pub fn read<T: Convert>(&self, addr: u32) -> T {
        let mut buf = [T::from(0)];
        self.read_slice(addr, &mut buf);

        debug_println!(
            "{:?} indirect memory read at {:x} with value {:x}",
            self.cpu_type,
            addr,
            buf[0].into()
        );

        buf[0]
    }

    pub fn read_slice<T: Convert>(&self, addr: u32, slice: &mut [T]) {
        let addr_end = addr + (slice.len() * mem::size_of::<T>()) as u32;

        let addr_base = addr & 0xFF000000;
        let addr_end_base = addr_end & 0xFF000000;
        debug_assert_eq!(addr_base, addr_end_base);

        let addr_offset = addr - addr_base;

        match addr_base {
            regions::MAIN_MEMORY_OFFSET => self
                .memory
                .read()
                .unwrap()
                .read_main_slice(addr_offset, slice),
            regions::SHARED_WRAM_OFFSET => {
                self.memory
                    .read()
                    .unwrap()
                    .read_wram_slice(self.cpu_type, addr_offset, slice)
            }
            regions::IO_PORTS_OFFSET => {
                for (i, value) in slice.iter_mut().enumerate() {
                    *value = self
                        .io_ports
                        .read(addr_offset + (i * mem::size_of::<T>()) as u32);
                }
            }
            _ => todo!(),
        };
    }

    pub fn write<T: Convert>(&mut self, addr: u32, value: T) {
        debug_println!(
            "{:?} indirect memory write at {:x} with value {:x}",
            self.cpu_type,
            addr,
            value.into(),
        );

        self.write_slice(addr, &[value]);
    }

    pub fn write_slice<T: Convert>(&mut self, addr: u32, slice: &[T]) {
        let addr_end = addr + (slice.len() * mem::size_of::<T>()) as u32;

        let addr_base = addr & 0xFF000000;
        let addr_end_base = addr_end & 0xFF000000;
        debug_assert_eq!(addr_base, addr_end_base);

        let addr_offset = addr - addr_base;

        match addr_base {
            regions::MAIN_MEMORY_OFFSET => self
                .memory
                .write()
                .unwrap()
                .write_main_slice(addr_offset, slice),
            regions::SHARED_WRAM_OFFSET => {
                self.memory
                    .write()
                    .unwrap()
                    .write_wram_slice(self.cpu_type, addr_offset, slice);

                for (i, _) in slice.iter().enumerate() {
                    self.invalidated_jit_addrs
                        .insert(addr + (i * mem::size_of::<T>()) as u32);
                }

                let (current_jit_block_start, current_jit_block_end) = self.current_jit_block_range;
                if addr >= current_jit_block_start && addr <= current_jit_block_end {
                    todo!()
                }
            }
            regions::IO_PORTS_OFFSET => {
                for (i, value) in slice.iter().enumerate() {
                    self.io_ports
                        .write(addr_offset + (i * mem::size_of::<T>()) as u32, *value);
                }
            }
            _ => todo!("{:x}", addr),
        };
    }
}

unsafe impl Send for MemHandler {}
unsafe impl Sync for MemHandler {}

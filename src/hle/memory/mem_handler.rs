use crate::hle::cp15_context::{Cp15Context, TcmState};
use crate::hle::memory::io_ports::IoPorts;
use crate::hle::memory::main_memory::MainMemory;
use crate::hle::memory::oam_context::OamContext;
use crate::hle::memory::palettes_context::PalettesContext;
use crate::hle::memory::regions;
use crate::hle::memory::tcm_context::TcmContext;
use crate::hle::memory::wram_context::WramContext;
use crate::hle::CpuType;
use crate::jit::jit_asm::JitState;
use crate::logging::debug_println;
use crate::utils::Convert;
use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

pub struct MemHandler<const CPU: CpuType> {
    main_memory: *mut MainMemory,
    wram_context: *mut WramContext,
    palettes_context: Rc<RefCell<PalettesContext>>,
    cp15_context: Rc<RefCell<Cp15Context>>,
    tcm_context: Rc<RefCell<TcmContext>>,
    pub io_ports: IoPorts<CPU>,
    pub jit_state: Rc<RefCell<JitState>>,
    oam: Rc<RefCell<OamContext>>,
}

unsafe impl<const CPU: CpuType> Send for MemHandler<CPU> {}
unsafe impl<const CPU: CpuType> Sync for MemHandler<CPU> {}

impl<const CPU: CpuType> MemHandler<CPU> {
    pub fn new(
        main_memory: *mut MainMemory,
        wram_context: Rc<RefCell<WramContext>>,
        palettes_context: Rc<RefCell<PalettesContext>>,
        cp15_context: Rc<RefCell<Cp15Context>>,
        tcm_context: Rc<RefCell<TcmContext>>,
        io_ports: IoPorts<CPU>,
        oam: Rc<RefCell<OamContext>>,
    ) -> Self {
        MemHandler {
            main_memory,
            wram_context: wram_context.as_ptr(),
            palettes_context,
            cp15_context,
            tcm_context,
            io_ports,
            jit_state: Rc::new(RefCell::new(JitState::new())),
            oam,
        }
    }

    pub fn read<T: Convert>(&self, mut addr: u32) -> T {
        debug_println!("{:?} memory read at {:x}", CPU, addr);
        addr &= !(mem::size_of::<T>() as u32 - 1);

        let addr_base = addr & 0xFF000000;
        let addr_offset = addr - addr_base;

        let ret = match addr_base {
            regions::MAIN_MEMORY_OFFSET => unsafe { (*self.main_memory).read(addr_offset) },
            regions::SHARED_WRAM_OFFSET => unsafe {
                (*self.wram_context).read::<CPU, _>(addr_offset)
            },
            regions::IO_PORTS_OFFSET => self.io_ports.read(addr_offset),
            regions::STANDARD_PALETTES_OFFSET => self.palettes_context.borrow().read(addr_offset),
            regions::VRAM_OFFSET => self
                .io_ports
                .vram_context
                .borrow()
                .read::<CPU, _>(addr_offset),
            regions::OAM_OFFSET => self.oam.borrow().read(addr_offset),
            _ => {
                let mut ret = T::from(0);

                if CPU == CpuType::ARM9 {
                    let cp15_context = self.cp15_context.borrow();
                    if addr < cp15_context.itcm_size {
                        if cp15_context.itcm_state == TcmState::RW {
                            ret = self.tcm_context.borrow_mut().read_itcm(addr);
                        }
                    } else if addr >= cp15_context.dtcm_addr
                        && addr < cp15_context.dtcm_addr + cp15_context.dtcm_size
                    {
                        if cp15_context.dtcm_state == TcmState::RW {
                            ret = self
                                .tcm_context
                                .borrow_mut()
                                .read_dtcm(addr - cp15_context.dtcm_addr);
                        }
                    } else {
                        todo!("{:x}", addr)
                    }
                } else {
                    todo!("{:x}", addr)
                }
                ret
            }
        };

        debug_println!(
            "{:?} memory read at {:x} with value {:x}",
            CPU,
            addr,
            ret.into()
        );

        ret
    }

    pub fn write<T: Convert>(&self, mut addr: u32, value: T) {
        debug_println!(
            "{:?} memory write at {:x} with value {:x}",
            CPU,
            addr,
            value.into(),
        );
        addr &= !(mem::size_of::<T>() as u32 - 1);

        let addr_base = addr & 0xFF000000;

        let addr_offset = addr - addr_base;
        let mut invalidate_jit = false;
        match addr_base {
            regions::MAIN_MEMORY_OFFSET => unsafe { (*self.main_memory).write(addr_offset, value) },
            regions::SHARED_WRAM_OFFSET => {
                invalidate_jit = true;
                unsafe { (*self.wram_context).write::<CPU, _>(addr_offset, value) }
            }
            regions::IO_PORTS_OFFSET => self.io_ports.write(addr_offset, value),
            regions::STANDARD_PALETTES_OFFSET => {
                self.palettes_context.borrow_mut().write(addr_offset, value)
            }
            regions::VRAM_OFFSET => self
                .io_ports
                .vram_context
                .borrow_mut()
                .write::<CPU, _>(addr_offset, value),
            regions::OAM_OFFSET => self.oam.borrow_mut().write(addr_offset, value),
            _ => {
                if CPU == CpuType::ARM9 {
                    let cp15_context = self.cp15_context.borrow();
                    if addr < cp15_context.itcm_size {
                        if cp15_context.itcm_state != TcmState::Disabled {
                            self.tcm_context.borrow_mut().write_itcm(addr, value);
                            invalidate_jit = true;
                        }
                    } else if addr >= cp15_context.dtcm_addr
                        && addr < cp15_context.dtcm_addr + cp15_context.dtcm_size
                    {
                        if cp15_context.dtcm_state != TcmState::Disabled {
                            self.tcm_context
                                .borrow_mut()
                                .write_dtcm(addr - cp15_context.dtcm_addr, value);
                        }
                    } else {
                        todo!("{:x}", addr)
                    }
                } else {
                    todo!("{:x}", addr)
                }
            }
        };

        if invalidate_jit {
            let mut jit_state = self.jit_state.borrow_mut();
            jit_state.invalidated_addrs.push(addr);

            #[cfg(debug_assertions)]
            {
                let (current_jit_block_start, current_jit_block_end) =
                    jit_state.current_block_range;
                if addr >= current_jit_block_start && addr <= current_jit_block_end {
                    todo!()
                }
            }
        }
    }
}

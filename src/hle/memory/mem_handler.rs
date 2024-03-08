use crate::hle::cp15_context::{Cp15Context, TcmState};
use crate::hle::memory::io_ports::IoPorts;
use crate::hle::memory::main_memory::MainMemory;
use crate::hle::memory::oam_context::OamContext;
use crate::hle::memory::palettes_context::PalettesContext;
use crate::hle::memory::regions;
use crate::hle::memory::tcm_context::TcmContext;
use crate::hle::memory::vram_context::VramContext;
use crate::hle::memory::wram_context::WramContext;
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils::Convert;
use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

pub struct MemHandler<const CPU: CpuType> {
    main_memory: *mut MainMemory,
    wram_context: *mut WramContext,
    palettes_context: *mut PalettesContext,
    cp15_context: *const Cp15Context,
    vram_context: *mut VramContext,
    tcm_context: *mut TcmContext,
    pub io_ports: IoPorts<CPU>,
    oam: *mut OamContext,
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
            palettes_context: palettes_context.as_ptr(),
            cp15_context: cp15_context.as_ptr(),
            vram_context: io_ports.vram_context.as_ptr(),
            tcm_context: tcm_context.as_ptr(),
            io_ports,
            oam: oam.as_ptr(),
        }
    }

    pub fn read<T: Convert>(&self, addr: u32) -> T {
        debug_println!("{:?} memory read at {:x}", CPU, addr);
        let aligned_addr = addr & !(mem::size_of::<T>() as u32 - 1);

        let addr_base = aligned_addr & 0xFF000000;
        let addr_offset = aligned_addr - addr_base;

        let ret = match addr_base {
            regions::MAIN_MEMORY_OFFSET => unsafe { (*self.main_memory).read(addr_offset) },
            regions::SHARED_WRAM_OFFSET => {
                if CPU == CpuType::ARM9 && {
                    let cp15_context = unsafe { self.cp15_context.as_ref().unwrap_unchecked() };
                    aligned_addr >= cp15_context.dtcm_addr
                        && aligned_addr < cp15_context.dtcm_addr + cp15_context.dtcm_size
                        && cp15_context.dtcm_state == TcmState::RW
                } {
                    unsafe {
                        (*self.tcm_context).read_dtcm(aligned_addr - (*self.cp15_context).dtcm_addr)
                    }
                } else {
                    unsafe { (*self.wram_context).read::<CPU, _>(addr_offset) }
                }
            }
            regions::IO_PORTS_OFFSET => self.io_ports.read(addr_offset),
            regions::STANDARD_PALETTES_OFFSET => unsafe {
                (*self.palettes_context).read(addr_offset)
            },
            regions::VRAM_OFFSET => unsafe { (*self.vram_context).read::<CPU, _>(addr_offset) },
            regions::OAM_OFFSET => unsafe { (*self.oam).read(addr_offset) },
            regions::GBA_ROM_OFFSET | regions::GBA_ROM_OFFSET2 | regions::GBA_RAM_OFFSET => {
                T::from(0xFFFFFFFF)
            }
            _ => {
                let mut ret = T::from(0);

                match CPU {
                    CpuType::ARM9 => {
                        let cp15_context = unsafe { self.cp15_context.as_ref().unwrap_unchecked() };
                        if aligned_addr < cp15_context.itcm_size {
                            if cp15_context.itcm_state == TcmState::RW {
                                ret = unsafe { (*self.tcm_context).read_itcm(aligned_addr) };
                            }
                        } else if aligned_addr >= cp15_context.dtcm_addr
                            && aligned_addr < cp15_context.dtcm_addr + cp15_context.dtcm_size
                        {
                            if cp15_context.dtcm_state == TcmState::RW {
                                ret = unsafe {
                                    (*self.tcm_context)
                                        .read_dtcm(aligned_addr - cp15_context.dtcm_addr)
                                }
                            }
                        } else if addr & regions::ARM9_BIOS_OFFSET == regions::ARM9_BIOS_OFFSET {
                            const HLE_BIOS: [u8; 8] = [0, 0, 0, 0xEC, 0, 0, 0, 0];
                            let addr = addr & (regions::ARM9_BIOS_SIZE - 1);
                            if addr as usize + 4 < HLE_BIOS.len() {
                                ret = T::from(u32::from_le_bytes(
                                    HLE_BIOS[addr as usize..addr as usize + 4]
                                        .try_into()
                                        .unwrap(),
                                ));
                            }
                        } else {
                            todo!("{:x} {:x}", aligned_addr, addr_base)
                        }
                    }
                    CpuType::ARM7 => {
                        if addr_base == regions::ARM7_BIOS_OFFSET {
                            const HLE_BIOS: [u8; 8] = [0, 0, 0, 0xEC, 0, 0, 0, 0];
                            let addr = addr & (regions::ARM7_BIOS_SIZE - 1);
                            if addr as usize + 4 < HLE_BIOS.len() {
                                ret = T::from(u32::from_le_bytes(
                                    HLE_BIOS[addr as usize..addr as usize + 4]
                                        .try_into()
                                        .unwrap(),
                                ));
                            }
                        } else {
                            todo!("{:x} {:x}", aligned_addr, addr_base)
                        }
                    }
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

    pub fn write<T: Convert>(&self, addr: u32, value: T) {
        debug_println!(
            "{:?} memory write at {:x} with value {:x}",
            CPU,
            addr,
            value.into(),
        );
        let aligned_addr = addr & !(mem::size_of::<T>() as u32 - 1);

        let addr_base = aligned_addr & 0xFF000000;
        let addr_offset = aligned_addr - addr_base;

        match addr_base {
            regions::MAIN_MEMORY_OFFSET => unsafe { (*self.main_memory).write(addr_offset, value) },
            regions::SHARED_WRAM_OFFSET => {
                if CPU == CpuType::ARM9 && {
                    let cp15_context = unsafe { self.cp15_context.as_ref().unwrap_unchecked() };
                    aligned_addr >= cp15_context.dtcm_addr
                        && aligned_addr < cp15_context.dtcm_addr + cp15_context.dtcm_size
                        && cp15_context.dtcm_state != TcmState::Disabled
                } {
                    unsafe {
                        (*self.tcm_context)
                            .write_dtcm(aligned_addr - (*self.cp15_context).dtcm_addr, value)
                    };
                } else {
                    unsafe { (*self.wram_context).write::<CPU, _>(addr_offset, value) };
                }
            }
            regions::IO_PORTS_OFFSET => self.io_ports.write(addr_offset, value),
            regions::STANDARD_PALETTES_OFFSET => unsafe {
                (*self.palettes_context).write(addr_offset, value)
            },
            regions::VRAM_OFFSET => unsafe {
                (*self.vram_context).write::<CPU, _>(addr_offset, value)
            },
            regions::OAM_OFFSET => unsafe { (*self.oam).write(addr_offset, value) },
            regions::GBA_ROM_OFFSET => {}
            _ => {
                if CPU == CpuType::ARM9 {
                    let cp15_context = unsafe { self.cp15_context.as_ref().unwrap_unchecked() };
                    if aligned_addr < cp15_context.itcm_size {
                        if cp15_context.itcm_state != TcmState::Disabled {
                            unsafe { (*self.tcm_context).write_itcm(aligned_addr, value) };
                        }
                    } else if aligned_addr >= cp15_context.dtcm_addr
                        && aligned_addr < cp15_context.dtcm_addr + cp15_context.dtcm_size
                        && cp15_context.dtcm_state != TcmState::Disabled
                    {
                        unsafe {
                            (*self.tcm_context)
                                .write_dtcm(aligned_addr - cp15_context.dtcm_addr, value)
                        };
                    } else {
                        todo!("{:?} {:x}", CPU, aligned_addr);
                    }
                } else {
                    todo!("{:?} {:x}", CPU, aligned_addr);
                }
            }
        };
    }
}

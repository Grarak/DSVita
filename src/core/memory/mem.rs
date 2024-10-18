use crate::core::cp15::TcmState;
use crate::core::emu::{get_cp15, Emu};
use crate::core::memory::io_arm7::IoArm7;
use crate::core::memory::io_arm9::IoArm9;
use crate::core::memory::mmu::{MmuArm7, MmuArm9};
use crate::core::memory::oam::Oam;
use crate::core::memory::palettes::Palettes;
use crate::core::memory::regions;
use crate::core::memory::vram::Vram;
use crate::core::memory::wram::Wram;
use crate::core::spu::SoundSampler;
use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::jit::jit_memory::{JitMemory, JitRegion};
use crate::logging::debug_println;
use crate::mmap::Shm;
use crate::utils::Convert;
use std::intrinsics::unlikely;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;
use CpuType::ARM7;

pub struct Memory {
    pub shm: Shm,
    pub wram: Wram,
    pub io_arm7: IoArm7,
    pub io_arm9: IoArm9,
    pub palettes: Palettes,
    pub vram: Vram,
    pub oam: Oam,
    pub jit: JitMemory,
    pub breakout_imm: bool,
    pub mmu_arm9: MmuArm9,
    pub mmu_arm7: MmuArm7,
}

macro_rules! get_mem_mmu {
    ($mem:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &$mem.mmu_arm9 as &dyn crate::core::memory::mmu::Mmu,
            crate::core::CpuType::ARM7 => &$mem.mmu_arm7 as &dyn crate::core::memory::mmu::Mmu,
        }
    }};
}

impl Memory {
    pub fn new(touch_points: Arc<AtomicU16>, sound_sampler: Arc<SoundSampler>) -> Self {
        Memory {
            shm: Shm::new("physical", regions::TOTAL_MEM_SIZE as usize).unwrap(),
            wram: Wram::new(),
            io_arm7: IoArm7::new(touch_points, sound_sampler),
            io_arm9: IoArm9::new(),
            palettes: Palettes::new(),
            vram: Vram::new(),
            oam: Oam::new(),
            jit: JitMemory::new(),
            breakout_imm: false,
            mmu_arm9: MmuArm9::new(),
            mmu_arm7: MmuArm7::new(),
        }
    }

    pub fn read<const CPU: CpuType, T: Convert>(&mut self, addr: u32, emu: &mut Emu) -> T {
        self.read_with_options::<CPU, true, T>(addr, emu)
    }

    pub fn read_no_tcm<const CPU: CpuType, T: Convert>(&mut self, addr: u32, emu: &mut Emu) -> T {
        self.read_with_options::<CPU, false, T>(addr, emu)
    }

    pub fn read_with_options<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, emu: &mut Emu) -> T {
        debug_println!("{:?} memory read at {:x}", CPU, addr);
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);

        let addr_base = aligned_addr & 0x0F000000;
        let addr_offset = aligned_addr & !0xFF000000;

        let vmem = {
            let mmu = get_mem_mmu!(self, CPU);
            if CPU == ARM9 && TCM {
                mmu.get_base_tcm_ptr()
            } else {
                mmu.get_base_ptr()
            }
        };

        if CPU == ARM9 && TCM {
            let cp15 = get_cp15!(emu, ARM9);
            if unlikely(aligned_addr >= cp15.dtcm_addr && aligned_addr < cp15.dtcm_addr + cp15.dtcm_size && cp15.dtcm_state == TcmState::RW) {
                let ret = unsafe { (vmem.add(aligned_addr as usize) as *const T).read() };
                debug_println!("{:?} dtcm read at {:x} with value {:x}", CPU, aligned_addr, ret.into());
                return ret;
            }
        }

        let ret = match addr_base {
            regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => match CPU {
                ARM9 => {
                    let mut ret = T::from(0);
                    if TCM {
                        let cp15 = get_cp15!(emu, ARM9);
                        if aligned_addr < cp15.itcm_size && cp15.itcm_state == TcmState::RW {
                            debug_println!("{:?} itcm read at {:x}", CPU, aligned_addr);
                            ret = unsafe { (vmem.add(aligned_addr as usize) as *const T).read() }
                        }
                    }
                    ret
                }
                // Bios of arm7 has same offset as itcm on arm9
                ARM7 => unsafe { (vmem.add(aligned_addr as usize) as *const T).read() },
            },
            regions::MAIN_OFFSET | regions::SHARED_WRAM_OFFSET | regions::GBA_ROM_OFFSET | regions::GBA_ROM_OFFSET2 | 0x0F000000 => unsafe {
                (vmem.add((aligned_addr & 0x0FFFFFFF) as usize) as *const T).read()
            },
            regions::IO_PORTS_OFFSET => match CPU {
                ARM9 => self.io_arm9.read(addr_offset, emu),
                ARM7 => {
                    if unlikely(addr_offset >= 0x800000) {
                        let addr_offset = addr_offset & !0x8000;
                        if unlikely(addr_offset >= 0x804000 && addr_offset < 0x806000) {
                            unsafe { (vmem.add(aligned_addr as usize) as *const T).read() }
                        } else {
                            self.io_arm7.read(addr_offset, emu)
                        }
                    } else {
                        self.io_arm7.read(addr_offset, emu)
                    }
                }
            },
            regions::STANDARD_PALETTES_OFFSET => self.palettes.read(addr_offset),
            regions::VRAM_OFFSET => self.vram.read::<CPU, _>(addr_offset),
            regions::OAM_OFFSET => self.oam.read(addr_offset),
            regions::GBA_RAM_OFFSET => T::from(0xFFFFFFFF),
            _ => unreachable!(),
        };

        debug_println!("{:?} memory read at {:x} with value {:x}", CPU, addr, ret.into());

        ret
    }

    pub fn write<const CPU: CpuType, T: Convert>(&mut self, addr: u32, value: T, emu: &mut Emu) {
        self.write_internal::<CPU, true, T>(addr, value, emu)
    }

    pub fn write_no_tcm<const CPU: CpuType, T: Convert>(&mut self, addr: u32, value: T, emu: &mut Emu) {
        self.write_internal::<CPU, false, T>(addr, value, emu)
    }

    fn write_internal<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, value: T, emu: &mut Emu) {
        debug_println!("{:?} memory write at {:x} with value {:x}", CPU, addr, value.into());
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);

        let addr_base = aligned_addr & 0x0F000000;
        let addr_offset = aligned_addr & !0xFF000000;

        let vmem = {
            let mmu = get_mem_mmu!(self, CPU);
            if CPU == ARM9 && TCM {
                mmu.get_base_tcm_ptr()
            } else {
                mmu.get_base_ptr()
            }
        };

        if CPU == ARM9 && TCM {
            let cp15 = get_cp15!(emu, ARM9);
            if unlikely(aligned_addr >= cp15.dtcm_addr && aligned_addr < cp15.dtcm_addr + cp15.dtcm_size && cp15.dtcm_state != TcmState::Disabled) {
                unsafe { (vmem.add(aligned_addr as usize) as *mut T).write(value) }
                debug_println!("{:?} dtcm write at {:x} with value {:x}", CPU, aligned_addr, value.into(),);
                return;
            }
        }

        match addr_base {
            regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => match CPU {
                ARM9 => {
                    if TCM {
                        let cp15 = get_cp15!(emu, ARM9);
                        if aligned_addr < cp15.itcm_size && cp15.itcm_state != TcmState::Disabled {
                            unsafe { (vmem.add(aligned_addr as usize) as *mut T).write(value) }
                            debug_println!("{:?} itcm write at {:x} with value {:x}", CPU, aligned_addr, value.into(),);
                            self.jit.invalidate_block::<{ JitRegion::Itcm }>(aligned_addr, size_of::<T>());
                        }
                    }
                }
                // Bios of arm7 has same offset as itcm on arm9
                ARM7 => {
                    // todo!("{:x} {:x}", aligned_addr, addr_base)
                }
            },
            regions::MAIN_OFFSET => {
                unsafe { (vmem.add(aligned_addr as usize) as *mut T).write(value) };
                self.jit.invalidate_block::<{ JitRegion::Main }>(aligned_addr, size_of::<T>());
            }
            regions::SHARED_WRAM_OFFSET => {
                unsafe { (vmem.add(aligned_addr as usize) as *mut T).write(value) };
                if CPU == ARM7 {
                    self.jit.invalidate_block::<{ JitRegion::Wram }>(aligned_addr, size_of::<T>());
                }
            }
            regions::IO_PORTS_OFFSET => match CPU {
                ARM9 => self.io_arm9.write(addr_offset, value, emu),
                ARM7 => {
                    if unlikely(addr_offset >= 0x800000) {
                        let addr_offset = addr_offset & !0x8000;
                        if unlikely(addr_offset >= 0x804000 && addr_offset < 0x806000) {
                            unsafe { (vmem.add(aligned_addr as usize) as *mut T).write(value) };
                        } else {
                            self.io_arm7.write(addr_offset, value, emu);
                        }
                    } else {
                        self.io_arm7.write(addr_offset, value, emu);
                    }
                }
            },
            regions::STANDARD_PALETTES_OFFSET => self.palettes.write(addr_offset, value),
            regions::VRAM_OFFSET => {
                self.vram.write::<CPU, _>(addr_offset, value);
                if CPU == ARM7 {
                    self.jit.invalidate_block::<{ JitRegion::VramArm7 }>(aligned_addr, size_of::<T>());
                }
            }
            regions::OAM_OFFSET => self.oam.write(addr_offset, value),
            regions::GBA_ROM_OFFSET => {}
            _ => unreachable!(),
        };
    }
}

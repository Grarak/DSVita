use crate::hle::cp15::TcmState;
use crate::hle::hle::{get_cp15, Hle};
use crate::hle::memory::io_arm7::IoArm7;
use crate::hle::memory::io_arm9::IoArm9;
use crate::hle::memory::main::Main;
use crate::hle::memory::oam::Oam;
use crate::hle::memory::palettes::Palettes;
use crate::hle::memory::regions;
use crate::hle::memory::tcm::Tcm;
use crate::hle::memory::vram::Vram;
use crate::hle::memory::wram::Wram;
use crate::hle::CpuType;
use crate::hle::CpuType::ARM9;
use crate::logging::debug_println;
use crate::utils::Convert;
use std::mem;
use CpuType::ARM7;

pub struct Memory {
    tcm: Tcm,
    pub main: Main,
    pub wram: Wram,
    pub io_arm7: IoArm7,
    pub io_arm9: IoArm9,
    pub palettes: Palettes,
    pub vram: Vram,
    pub oam: Oam,
}

impl Memory {
    pub fn new() -> Self {
        Memory {
            tcm: Tcm::new(),
            main: Main::new(),
            wram: Wram::new(),
            io_arm7: IoArm7::new(),
            io_arm9: IoArm9::new(),
            palettes: Palettes::new(),
            vram: Vram::new(),
            oam: Oam::new(),
        }
    }

    pub fn read<const CPU: CpuType, T: Convert>(&mut self, addr: u32, hle: &mut Hle) -> T {
        self.read_internal::<CPU, true, T>(addr, hle)
    }

    pub fn read_no_tcm<const CPU: CpuType, T: Convert>(&mut self, addr: u32, hle: &mut Hle) -> T {
        self.read_internal::<CPU, false, T>(addr, hle)
    }

    fn read_internal<const CPU: CpuType, const TCM: bool, T: Convert>(
        &mut self,
        addr: u32,
        hle: &mut Hle,
    ) -> T {
        debug_println!("{:?} memory read at {:x}", CPU, addr);
        let aligned_addr = addr & !(mem::size_of::<T>() as u32 - 1);

        let addr_base = aligned_addr & 0xFF000000;
        let addr_offset = aligned_addr - addr_base;

        let ret = match addr_base {
            regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => match CPU {
                ARM9 => {
                    let mut ret = T::from(0);
                    if TCM {
                        let cp15 = get_cp15!(hle, ARM9);
                        if addr_offset < cp15.itcm_size {
                            if cp15.itcm_state == TcmState::RW {
                                ret = self.tcm.read_itcm(addr_offset);
                            }
                        } else if aligned_addr >= cp15.dtcm_addr
                            && aligned_addr < cp15.dtcm_addr + cp15.dtcm_size
                        {
                            if cp15.dtcm_state == TcmState::RW {
                                ret = self.tcm.read_dtcm(aligned_addr - cp15.dtcm_addr);
                            }
                        } else {
                            todo!("{:x} {:x}", aligned_addr, addr_base)
                        }
                    } else {
                        // todo!("{:x} {:x}", aligned_addr, addr_base)
                    }
                    ret
                }
                // Bios of arm7 has same offset as itcm on arm9
                ARM7 => {
                    if aligned_addr < regions::ARM7_BIOS_SIZE {
                        const HLE_BIOS: [u8; 8] = [0, 0, 0, 0xEC, 0, 0, 0, 0];
                        if aligned_addr as usize + 4 < HLE_BIOS.len() {
                            T::from(u32::from_le_bytes(
                                HLE_BIOS[aligned_addr as usize..addr as usize + 4]
                                    .try_into()
                                    .unwrap(),
                            ))
                        } else {
                            T::from(0)
                        }
                    } else {
                        todo!("{:x} {:x}", aligned_addr, addr_base)
                    }
                }
            },
            regions::MAIN_MEMORY_OFFSET => self.main.read(addr_offset),
            regions::SHARED_WRAM_OFFSET => {
                if CPU == ARM9 && TCM && {
                    let cpi15 = get_cp15!(hle, ARM9);
                    aligned_addr >= cpi15.dtcm_addr
                        && aligned_addr < cpi15.dtcm_addr + cpi15.dtcm_size
                        && cpi15.dtcm_state == TcmState::RW
                } {
                    self.tcm
                        .read_dtcm(aligned_addr - get_cp15!(hle, ARM9).dtcm_addr)
                } else {
                    self.wram.read::<CPU, _>(addr_offset)
                }
            }
            regions::IO_PORTS_OFFSET => match CPU {
                ARM9 => self.io_arm9.read(addr_offset, hle),
                ARM7 => self.io_arm7.read(addr_offset, hle),
            },
            regions::STANDARD_PALETTES_OFFSET => self.palettes.read(addr_offset),
            regions::VRAM_OFFSET => self.vram.read::<CPU, _>(addr_offset),
            regions::OAM_OFFSET => self.oam.read(addr_offset),
            regions::GBA_ROM_OFFSET | regions::GBA_ROM_OFFSET2 | regions::GBA_RAM_OFFSET => {
                T::from(0xFFFFFFFF)
            }
            0xFF000000 => match CPU {
                ARM9 => {
                    if (aligned_addr & 0xFFFF8000) == regions::ARM9_BIOS_OFFSET {
                        const HLE_BIOS: [u8; 8] = [0, 0, 0, 0xEC, 0, 0, 0, 0];
                        let aligned_addr = aligned_addr & (regions::ARM9_BIOS_SIZE - 1);
                        if aligned_addr as usize + 4 < HLE_BIOS.len() {
                            T::from(u32::from_le_bytes(
                                HLE_BIOS[aligned_addr as usize..aligned_addr as usize + 4]
                                    .try_into()
                                    .unwrap(),
                            ))
                        } else {
                            T::from(0)
                        }
                    } else {
                        todo!("{:x} {:x}", aligned_addr, addr_base)
                    }
                }
                ARM7 => {
                    todo!("{:x} {:x}", aligned_addr, addr_base)
                }
            },
            _ => {
                let mut ret = T::from(0);
                if CPU == ARM9 && TCM {
                    let cp15 = get_cp15!(hle, ARM9);
                    if aligned_addr >= cp15.dtcm_addr
                        && aligned_addr < cp15.dtcm_addr + cp15.dtcm_size
                    {
                        if cp15.dtcm_state == TcmState::RW {
                            ret = self.tcm.read_dtcm(aligned_addr - cp15.dtcm_addr);
                        }
                    } else {
                        todo!("{:x} {:x}", aligned_addr, addr_base)
                    }
                } else {
                    todo!("{:x} {:x}", aligned_addr, addr_base)
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

    pub fn write<const CPU: CpuType, T: Convert>(&mut self, addr: u32, value: T, hle: &mut Hle) {
        self.write_internal::<CPU, true, T>(addr, value, hle)
    }

    pub fn write_no_tcm<const CPU: CpuType, T: Convert>(
        &mut self,
        addr: u32,
        value: T,
        hle: &mut Hle,
    ) {
        self.write_internal::<CPU, false, T>(addr, value, hle)
    }

    fn write_internal<const CPU: CpuType, const TCM: bool, T: Convert>(
        &mut self,
        addr: u32,
        value: T,
        hle: &mut Hle,
    ) {
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
            regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => match CPU {
                ARM9 => {
                    if TCM {
                        let cp15 = get_cp15!(hle, ARM9);
                        if addr_offset < cp15.itcm_size {
                            if cp15.itcm_state != TcmState::Disabled {
                                self.tcm.write_itcm(addr_offset, value);
                            }
                        } else if aligned_addr >= cp15.dtcm_addr
                            && aligned_addr < cp15.dtcm_addr + cp15.dtcm_size
                        {
                            if cp15.dtcm_state != TcmState::Disabled {
                                self.tcm.write_dtcm(aligned_addr - cp15.dtcm_addr, value);
                            }
                        }
                    } else {
                        // todo!("{:x} {:x}", aligned_addr, addr_base)
                    }
                }
                // Bios of arm7 has same offset as itcm on arm9
                ARM7 => {
                    todo!("{:x} {:x}", aligned_addr, addr_base)
                }
            },
            regions::MAIN_MEMORY_OFFSET => self.main.write(addr_offset, value),
            regions::SHARED_WRAM_OFFSET => {
                if CPU == ARM9 && TCM && {
                    let cp15_context = get_cp15!(hle, ARM9);
                    aligned_addr >= cp15_context.dtcm_addr
                        && aligned_addr < cp15_context.dtcm_addr + cp15_context.dtcm_size
                        && cp15_context.dtcm_state != TcmState::Disabled
                } {
                    self.tcm
                        .write_dtcm(aligned_addr - get_cp15!(hle, ARM9).dtcm_addr, value);
                } else {
                    self.wram.write::<CPU, _>(addr_offset, value);
                }
            }
            regions::IO_PORTS_OFFSET => match CPU {
                ARM9 => self.io_arm9.write(addr_offset, value, hle),
                ARM7 => self.io_arm7.write(addr_offset, value, hle),
            },
            regions::STANDARD_PALETTES_OFFSET => self.palettes.write(addr_offset, value),
            regions::VRAM_OFFSET => self.vram.write::<CPU, _>(addr_offset, value),
            regions::OAM_OFFSET => self.oam.write(addr_offset, value),
            regions::GBA_ROM_OFFSET => {}
            _ => {
                if CPU == ARM9 && TCM {
                    let cp15 = get_cp15!(hle, ARM9);
                    if aligned_addr >= cp15.dtcm_addr
                        && aligned_addr < cp15.dtcm_addr + cp15.dtcm_size
                    {
                        if cp15.dtcm_state != TcmState::Disabled {
                            self.tcm.write_dtcm(aligned_addr - cp15.dtcm_addr, value);
                        }
                    } else {
                        todo!("{:x} {:x}", aligned_addr, addr_base)
                    }
                } else {
                    todo!("{:x} {:x}", aligned_addr, addr_base)
                }
            }
        };
    }
}

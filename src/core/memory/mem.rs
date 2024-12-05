use crate::core::cp15::TcmState;
use crate::core::emu::{get_cp15, Emu};
use crate::core::memory::io_arm7::IoArm7;
use crate::core::memory::io_arm9::IoArm9;
use crate::core::memory::mmu::{MmuArm7, MmuArm9, MMU_PAGE_SHIFT, MMU_PAGE_SIZE};
use crate::core::memory::oam::Oam;
use crate::core::memory::palettes::Palettes;
use crate::core::memory::regions;
use crate::core::memory::vram::Vram;
use crate::core::memory::wifi::Wifi;
use crate::core::memory::wram::Wram;
use crate::core::spu::SoundSampler;
use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::jit::jit_memory::{JitMemory, JitRegion};
use crate::logging::debug_println;
use crate::mmap::Shm;
use crate::utils::Convert;
use crate::{utils, IS_DEBUG};
use std::hint::unreachable_unchecked;
use std::intrinsics::unlikely;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;
use CpuType::ARM7;

pub struct Memory {
    pub shm: Shm,
    pub wram: Wram,
    pub io_arm7: IoArm7,
    pub io_arm9: IoArm9,
    pub wifi: Wifi,
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

macro_rules! slow_read {
    ($self:expr, $cpu:expr, $tcm:expr, $emu:expr, $addr:expr, $ret:expr, $shm_offset:ident, $addr_offset:ident, $shm_read:block, $io_read:block, $wifi_read:block, $palettes_read:block, $vram_read:block, $oam_read:block, $gba_read:block, $zero_read:block) => {{
        let addr_base = $addr & 0x0F000000;
        let $addr_offset = $addr & 0x00FFFFFF;

        if $cpu == ARM9 && $tcm {
            let cp15 = get_cp15!($emu, ARM9);
            if unlikely($addr >= cp15.dtcm_addr && $addr < cp15.dtcm_addr + cp15.dtcm_size && cp15.dtcm_state == TcmState::RW) {
                let dtcm_addr = $addr - cp15.dtcm_addr;
                let $shm_offset = regions::DTCM_REGION.shm_offset as u32 + (dtcm_addr & (regions::DTCM_SIZE - 1));
                $shm_read;
                return $ret;
            }
        }

        match addr_base {
            regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => match CPU {
                ARM9 => {
                    if $tcm {
                        let cp15 = get_cp15!($emu, ARM9);
                        if $addr < cp15.itcm_size && cp15.itcm_state == TcmState::RW {
                            debug_println!("{:?} itcm read at {:x}", CPU, $addr);
                            let $shm_offset = regions::ITCM_REGION.shm_offset as u32 + ($addr & (regions::ITCM_SIZE - 1));
                            $shm_read;
                        }
                    }
                }
                // Bios of arm7 has same offset as itcm on arm9
                ARM7 => { $zero_read; }
            },
            regions::MAIN_OFFSET => {
                let $shm_offset = regions::MAIN_REGION.shm_offset as u32 + ($addr & (regions::MAIN_SIZE - 1));
                $shm_read;
            }
            regions::SHARED_WRAM_OFFSET => {
                let $shm_offset = $self.wram.get_shm_offset::<CPU>($addr) as u32;
                $shm_read;
            }
            regions::IO_PORTS_OFFSET => match CPU {
                ARM9 => {
                    $io_read;
                }
                ARM7 => {
                    if unlikely($addr_offset >= 0x800000) {
                        let $addr_offset = $addr_offset & !0x8000;
                        if unlikely((0x804000..0x806000).contains(&$addr_offset)) {
                            $wifi_read;
                        } else {
                            $io_read;
                        }
                    } else {
                        $io_read;
                    }
                }
            },
            regions::STANDARD_PALETTES_OFFSET => {
                $palettes_read;
            }
            regions::VRAM_OFFSET => {
                $vram_read;
            }
            regions::OAM_OFFSET => {
                $oam_read;
            }
            regions::GBA_ROM_OFFSET | regions::GBA_ROM_OFFSET2 | regions::GBA_RAM_OFFSET => {
                $gba_read;
            }
            0x0F000000 => match CPU {
                ARM9 => {
                    $zero_read;
                }
                ARM7 => if IS_DEBUG {
                    unreachable!("{CPU:?} {:x} tcm: {}", $addr, $tcm)
                } else {
                    unsafe { unreachable_unchecked() }
                },
            },
            _ => {
                if IS_DEBUG {
                    unreachable!("{CPU:?} {:x} tcm: {}", $addr, $tcm)
                } else {
                    unsafe { unreachable_unchecked() }
                }
            }
        };

        $ret
    }};
}

macro_rules! slow_write {
    ($self:expr, $cpu:expr, $tcm:expr, $emu:expr, $addr:expr, $size:expr, $shm_offset:ident, $addr_offset:ident, $shm_write:block, $io_write:block, $wifi_write:block, $palettes_write:block, $vram_write:block, $oam_write:block) => {{
        if $cpu == ARM9 && $tcm {
            let cp15 = get_cp15!($emu, ARM9);
            if unlikely($addr >= cp15.dtcm_addr && $addr < cp15.dtcm_addr + cp15.dtcm_size && cp15.dtcm_state != TcmState::Disabled) {
                let dtcm_addr = $addr - cp15.dtcm_addr;
                let $shm_offset = regions::DTCM_REGION.shm_offset as u32 + (dtcm_addr & (regions::DTCM_SIZE - 1));
                $shm_write;
                return;
            }
        }

        let addr_base = $addr & 0x0F000000;
        let $addr_offset = $addr & !0xFF000000;

        match addr_base {
            regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => match $cpu {
                ARM9 => {
                    if $tcm {
                        let cp15 = get_cp15!($emu, ARM9);
                        if $addr < cp15.itcm_size && cp15.itcm_state != TcmState::Disabled {
                            let $shm_offset = regions::ITCM_REGION.shm_offset as u32 + ($addr_offset & (regions::ITCM_SIZE - 1));
                            $shm_write;
                            $self.jit.invalidate_block::<{ JitRegion::Itcm }>($addr, $size);
                        }
                    }
                }
                // Bios of arm7 has same offset as itcm on arm9
                ARM7 => {}
            },
            regions::MAIN_OFFSET => {
                let $shm_offset = regions::MAIN_REGION.shm_offset as u32 + ($addr_offset & (regions::MAIN_SIZE - 1));
                $shm_write;
                $self.jit.invalidate_block::<{ JitRegion::Main }>($addr, $size);
            }
            regions::SHARED_WRAM_OFFSET => {
                let $shm_offset = $self.wram.get_shm_offset::<$cpu>($addr) as u32;
                $shm_write;
                if $cpu == ARM7 {
                    $self.jit.invalidate_block::<{ JitRegion::Wram }>($addr, $size);
                }
            }
            regions::IO_PORTS_OFFSET => match $cpu {
                ARM9 => {
                    $io_write;
                }
                ARM7 => {
                    if unlikely($addr_offset >= 0x800000) {
                        let $addr_offset = $addr_offset & !0x8000;
                        if unlikely((0x804000..0x806000).contains(&$addr_offset)) {
                            $wifi_write;
                        } else {
                            $io_write;
                        }
                    } else {
                        $io_write;
                    }
                }
            },
            regions::STANDARD_PALETTES_OFFSET => {
                $palettes_write;
            }
            regions::VRAM_OFFSET => {
                $vram_write;
                if $cpu == ARM7 {
                    $self.jit.invalidate_block::<{ JitRegion::VramArm7 }>($addr, $size);
                }
            }
            regions::OAM_OFFSET => {
                $oam_write;
            }
            regions::GBA_ROM_OFFSET => {}
            _ => unsafe { unreachable_unchecked() },
        };
    }};
}

impl Memory {
    pub fn new(touch_points: Arc<AtomicU16>, sound_sampler: Arc<SoundSampler>) -> Self {
        Memory {
            shm: Shm::new("physical", regions::TOTAL_MEM_SIZE as usize).unwrap(),
            wram: Wram::new(),
            io_arm7: IoArm7::new(touch_points, sound_sampler),
            io_arm9: IoArm9::new(),
            wifi: Wifi::new(),
            palettes: Palettes::new(),
            vram: Vram::new(),
            oam: Oam::new(),
            jit: JitMemory::new(),
            breakout_imm: false,
            mmu_arm9: MmuArm9::new(),
            mmu_arm7: MmuArm7::new(),
        }
    }

    pub fn get_shm_offset<const CPU: CpuType, const TCM: bool, const WRITE: bool>(&self, addr: u32) -> usize {
        let mmu = {
            let mmu = get_mem_mmu!(self, CPU);
            if CPU == ARM9 && TCM {
                if WRITE {
                    mmu.get_mmu_write_tcm()
                } else {
                    mmu.get_mmu_read_tcm()
                }
            } else if WRITE {
                mmu.get_mmu_write()
            } else {
                mmu.get_mmu_read()
            }
        };

        let shm_offset = unsafe { *mmu.get_unchecked((addr as usize) >> MMU_PAGE_SHIFT) };
        if shm_offset != 0 {
            let offset = (addr as usize) & (MMU_PAGE_SIZE - 1);
            shm_offset + offset
        } else {
            0
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
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        {
            let shm_offset = self.get_shm_offset::<CPU, TCM, false>(aligned_addr) as u32;
            if shm_offset != 0 {
                return utils::read_from_mem(&self.shm, shm_offset);
            }
        }

        let mut ret = T::from(0);
        slow_read!(
            self,
            CPU,
            TCM,
            emu,
            aligned_addr,
            ret,
            shm_offset,
            addr_offset,
            { ret = utils::read_from_mem(&self.shm, shm_offset) },
            {
                ret = match CPU {
                    ARM9 => self.io_arm9.read(addr_offset, emu),
                    ARM7 => self.io_arm7.read(addr_offset, emu),
                }
            },
            { ret = self.wifi.read(addr_offset) },
            { ret = self.palettes.read(addr_offset) },
            { ret = self.vram.read::<CPU, _>(addr_offset) },
            { ret = self.oam.read(addr_offset) },
            { ret = T::from(0xFFFFFFFF) },
            { ret = T::from(0) }
        )
    }

    pub fn read_multiple<const CPU: CpuType, T: Convert, F: FnMut(T)>(&mut self, addr: u32, emu: &mut Emu, size: usize, mut write_value: F) {
        debug_println!("{CPU:?} multiple memory read at {addr:x} with size {size}");
        let read_shift = size_of::<T>() >> 1;
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        {
            let shm_offset = self.get_shm_offset::<CPU, true, false>(aligned_addr) as u32;
            if shm_offset != 0 {
                for i in 0..size {
                    write_value(utils::read_from_mem(&self.shm, shm_offset + (i << read_shift) as u32));
                }
                return;
            }
        }

        let fn_ret = ();
        slow_read!(
            self,
            CPU,
            true,
            emu,
            aligned_addr,
            fn_ret,
            shm_offset,
            addr_offset,
            {
                for i in 0..size {
                    write_value(utils::read_from_mem(&self.shm, shm_offset + (i << read_shift) as u32));
                }
            },
            {
                for i in 0..size {
                    write_value(match CPU {
                        ARM9 => self.io_arm9.read(addr_offset + (i << read_shift) as u32, emu),
                        ARM7 => self.io_arm7.read(addr_offset + (i << read_shift) as u32, emu),
                    });
                }
            },
            {
                for i in 0..size {
                    write_value(self.wifi.read(addr_offset + (i << read_shift) as u32));
                }
            },
            {
                for i in 0..size {
                    write_value(self.palettes.read(addr_offset + (i << read_shift) as u32));
                }
            },
            {
                for i in 0..size {
                    write_value(self.vram.read::<CPU, _>(addr_offset + (i << read_shift) as u32));
                }
            },
            {
                for i in 0..size {
                    write_value(self.oam.read(addr_offset + (i << read_shift) as u32));
                }
            },
            {
                for i in 0..size {
                    write_value(T::from(0xFFFFFFFF));
                }
            },
            {
                for i in 0..size {
                    write_value(T::from(0));
                }
            }
        );
    }

    pub fn read_multiple_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, emu: &mut Emu, slice: &mut [T]) {
        debug_println!("{CPU:?} slice memory read at {addr:x} with size {}", size_of_val(slice));
        let read_shift = size_of::<T>() >> 1;
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        {
            let shm_offset = self.get_shm_offset::<CPU, TCM, false>(aligned_addr) as u32;
            if shm_offset != 0 {
                utils::read_from_mem_slice(&self.shm, shm_offset, slice);
                return;
            }
        }

        let fn_ret = ();
        slow_read!(
            self,
            CPU,
            TCM,
            emu,
            aligned_addr,
            fn_ret,
            shm_offset,
            addr_offset,
            { utils::read_from_mem_slice(&self.shm, shm_offset, slice) },
            {
                for i in 0..slice.len() {
                    slice[i] = match CPU {
                        ARM9 => self.io_arm9.read(addr_offset + (i << read_shift) as u32, emu),
                        ARM7 => self.io_arm7.read(addr_offset + (i << read_shift) as u32, emu),
                    };
                }
            },
            { self.wifi.read_slice(addr_offset, slice) },
            { self.palettes.read_slice(addr_offset, slice) },
            {
                for i in 0..slice.len() {
                    slice[i] = self.vram.read::<CPU, _>(addr_offset + (i << read_shift) as u32);
                }
            },
            { self.oam.read_slice(addr_offset, slice) },
            { slice.fill(T::from(0xFFFFFFFF)) },
            { slice.fill(T::from(0)) }
        );
    }

    pub fn read_fixed_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, emu: &mut Emu, slice: &mut [T]) {
        debug_println!("{CPU:?} fixed slice memory read at {addr:x} with size {}", size_of_val(slice));
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        {
            let shm_offset = self.get_shm_offset::<CPU, TCM, false>(aligned_addr) as u32;
            if shm_offset != 0 {
                slice.fill(utils::read_from_mem(&self.shm, shm_offset));
                return;
            }
        }

        let fn_ret = ();
        slow_read!(
            self,
            CPU,
            TCM,
            emu,
            aligned_addr,
            fn_ret,
            shm_offset,
            addr_offset,
            { slice.fill(utils::read_from_mem(&self.shm, shm_offset)) },
            {
                for i in 0..slice.len() {
                    slice[i] = match CPU {
                        ARM9 => self.io_arm9.read(addr_offset, emu),
                        ARM7 => self.io_arm7.read(addr_offset, emu),
                    };
                }
            },
            { slice.fill(self.wifi.read(addr_offset)) },
            { slice.fill(self.palettes.read(addr_offset)) },
            { slice.fill(self.vram.read::<CPU, _>(addr_offset)) },
            { slice.fill(self.oam.read(addr_offset)) },
            { slice.fill(T::from(0xFFFFFFFF)) },
            { slice.fill(T::from(0)) }
        );
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
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr);
        if shm_offset != 0 {
            utils::write_to_mem(&mut self.shm, shm_offset as u32, value);
            return;
        }

        slow_write!(
            self,
            CPU,
            TCM,
            emu,
            aligned_addr,
            size_of::<T>(),
            shm_offset,
            addr_offset,
            { utils::write_to_mem(&mut self.shm, shm_offset, value) },
            {
                match CPU {
                    ARM9 => self.io_arm9.write(addr_offset, value, emu),
                    ARM7 => self.io_arm7.write(addr_offset, value, emu),
                }
            },
            { self.wifi.write(addr_offset, value) },
            { self.palettes.write(addr_offset, value) },
            { self.vram.write::<CPU, _>(addr_offset, value) },
            { self.oam.write(addr_offset, value) }
        );
    }

    pub fn write_multiple<const CPU: CpuType, T: Convert, F: FnMut() -> T>(&mut self, addr: u32, emu: &mut Emu, size: usize, mut get_value: F) {
        debug_println!("{CPU:?} multiple memory write at {addr:x} with size {size}");
        let write_shift = size_of::<T>() >> 1;
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, true, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            for i in 0..size {
                utils::write_to_mem(&mut self.shm, shm_offset + (i << write_shift) as u32, get_value());
            }
            return;
        }

        slow_write!(
            self,
            CPU,
            true,
            emu,
            aligned_addr,
            size << write_shift,
            shm_offset,
            addr_offset,
            {
                for i in 0..size {
                    utils::write_to_mem(&mut self.shm, shm_offset + (i << write_shift) as u32, get_value());
                }
            },
            {
                for i in 0..size {
                    match CPU {
                        ARM9 => self.io_arm9.write(addr_offset + (i << write_shift) as u32, get_value(), emu),
                        ARM7 => self.io_arm7.write(addr_offset + (i << write_shift) as u32, get_value(), emu),
                    }
                }
            },
            {
                for i in 0..size {
                    self.wifi.write(addr_offset + (i << write_shift) as u32, get_value());
                }
            },
            {
                for i in 0..size {
                    self.palettes.write(addr_offset + (i << write_shift) as u32, get_value());
                }
            },
            {
                for i in 0..size {
                    self.vram.write::<CPU, _>(addr_offset + (i << write_shift) as u32, get_value());
                }
            },
            {
                for i in 0..size {
                    self.oam.write(addr_offset + (i << write_shift) as u32, get_value());
                }
            }
        );
    }

    pub fn write_multiple_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, emu: &mut Emu, slice: &[T]) {
        debug_println!("{CPU:?} fixed slice memory write at {addr:x} with size {}", slice.len());
        let write_shift = size_of::<T>() >> 1;
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            utils::write_to_mem_slice(&mut self.shm, shm_offset as usize, slice);
            return;
        }

        slow_write!(
            self,
            CPU,
            TCM,
            emu,
            aligned_addr,
            size_of_val(slice),
            shm_offset,
            addr_offset,
            { utils::write_to_mem_slice(&mut self.shm, shm_offset as usize, slice) },
            {
                for i in 0..slice.len() {
                    match CPU {
                        ARM9 => self.io_arm9.write(addr_offset + (i << write_shift) as u32, slice[i], emu),
                        ARM7 => self.io_arm7.write(addr_offset + (i << write_shift) as u32, slice[i], emu),
                    }
                }
            },
            { self.wifi.write_slice(addr_offset, slice) },
            { self.palettes.write_slice(addr_offset, slice) },
            {
                for i in 0..slice.len() {
                    self.vram.write::<CPU, _>(addr_offset + (i << write_shift) as u32, slice[i]);
                }
            },
            { self.oam.write_slice(addr_offset, slice) }
        );
    }

    pub fn write_fixed_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, emu: &mut Emu, slice: &[T]) {
        debug_println!("{CPU:?} fixed slice memory write at {addr:x} with size {}", slice.len());
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            utils::write_to_mem(&mut self.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() });
            return;
        }

        slow_write!(
            self,
            CPU,
            TCM,
            emu,
            aligned_addr,
            size_of_val(slice),
            shm_offset,
            addr_offset,
            { utils::write_to_mem(&mut self.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() }) },
            {
                match CPU {
                    ARM9 => self.io_arm9.write_fixed_slice(addr_offset, slice, emu),
                    ARM7 => {
                        for i in 0..slice.len() {
                            self.io_arm7.write(addr_offset, slice[i], emu);
                        }
                    }
                }
            },
            { self.wifi.write(addr_offset, unsafe { *slice.last().unwrap_unchecked() }) },
            { self.palettes.write(addr_offset, unsafe { *slice.last().unwrap_unchecked() }) },
            {
                for i in 0..slice.len() {
                    self.vram.write::<CPU, _>(addr_offset, slice[i]);
                }
            },
            { self.oam.write(addr_offset, unsafe { *slice.last().unwrap_unchecked() }) }
        );
    }

    pub fn write_multiple_memset<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, value: T, size: usize, emu: &mut Emu) {
        debug_println!("{CPU:?} multiple memset memory write at {addr:x} with size {size}");
        let write_shift = size_of::<T>() >> 1;
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            utils::write_memset(&mut self.shm, shm_offset as usize, value, size);
            return;
        }

        slow_write!(
            self,
            CPU,
            TCM,
            emu,
            aligned_addr,
            size << write_shift,
            shm_offset,
            addr_offset,
            { utils::write_memset(&mut self.shm, shm_offset as usize, value, size) },
            {
                for i in 0..size {
                    match CPU {
                        ARM9 => self.io_arm9.write(addr_offset + (i << write_shift) as u32, value, emu),
                        ARM7 => self.io_arm7.write(addr_offset + (i << write_shift) as u32, value, emu),
                    }
                }
            },
            { self.wifi.write_memset(addr_offset, value, size) },
            { self.palettes.write_memset(addr_offset, value, size) },
            {
                for i in 0..size {
                    self.vram.write::<CPU, _>(addr_offset + (i << write_shift) as u32, value);
                }
            },
            { self.oam.write_memset(addr_offset, value, size) }
        );
    }
}

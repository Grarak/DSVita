use crate::core::cp15::TcmState;
use crate::core::emu::{get_cp15, get_jit_mut, get_mem, get_mem_mut, Emu};
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
use crate::jit::jit_memory::JitMemory;
use crate::logging::debug_println;
use crate::mmap::Shm;
use crate::utils;
use crate::utils::Convert;
use std::hint::unreachable_unchecked;
use std::intrinsics::unlikely;
use std::marker::PhantomData;
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

macro_rules! create_io_read_lut {
    () => {
        [
            Self::read_itcm,
            Self::read_itcm,
            Self::read_main,
            Self::read_wram,
            Self::read_io_ports,
            Self::read_palettes,
            Self::read_vram,
            Self::read_oam,
            Self::read_gba,
            Self::read_gba,
            Self::read_gba,
            Self::read_invalid,
            Self::read_invalid,
            Self::read_invalid,
            Self::read_invalid,
            Self::read_bios,
        ]
    };
}

macro_rules! create_io_write_lut {
    () => {
        [
            Self::write_itcm,
            Self::write_itcm,
            Self::write_main,
            Self::write_wram,
            Self::write_io_ports,
            Self::write_palettes,
            Self::write_vram,
            Self::write_oam,
            Self::write_gba,
        ]
    };
}

macro_rules! read_dtcm {
    ($cpu:expr, $tcm:expr, $addr:expr, $emu:expr, $mem:ident, $shm_offset:ident, $read:block) => {{
        if $cpu == ARM9 && $tcm {
            let cp15 = get_cp15!($emu);
            if unlikely($addr >= cp15.dtcm_addr && $addr < cp15.dtcm_addr + cp15.dtcm_size && cp15.dtcm_state == TcmState::RW) {
                let dtcm_addr = $addr - cp15.dtcm_addr;
                let $shm_offset = regions::DTCM_REGION.shm_offset as u32 + (dtcm_addr & (regions::DTCM_SIZE - 1));
                let $mem = get_mem!($emu);
                return $read;
            }
        }
    }};
}

macro_rules! read_itcm {
    ($cpu:expr, $tcm:expr, $addr:expr, $emu:expr, $mem:ident, $shm_offset:ident, $read:block, $read_zero:block) => {{
        match $cpu {
            ARM9 => {
                if $tcm {
                    let cp15 = get_cp15!($emu);
                    if $addr < cp15.itcm_size && cp15.itcm_state == TcmState::RW {
                        debug_println!("{:?} itcm read at {:x}", $cpu, $addr);
                        let $shm_offset = regions::ITCM_REGION.shm_offset as u32 + ($addr & (regions::ITCM_SIZE - 1));
                        let $mem = get_mem!($emu);
                        return $read;
                    }
                }
                $read_zero
            }
            ARM7 => $read_zero,
        }
    }};
}

macro_rules! read_main {
    ($addr:expr, $emu:expr, $mem:ident, $shm_offset:ident, $read:block) => {{
        let $shm_offset = regions::MAIN_REGION.shm_offset as u32 + ($addr & (regions::MAIN_SIZE - 1));
        let $mem = get_mem!($emu);
        $read
    }};
}

macro_rules! read_wram {
    ($cpu:expr, $addr:expr, $emu:expr, $mem:ident, $shm_offset:ident, $read:block) => {{
        let $mem = get_mem!($emu);
        let $shm_offset = $mem.wram.get_shm_offset::<{ $cpu }>($addr) as u32;
        $read
    }};
}

macro_rules! read_io_ports {
    ($cpu:expr, $addr:expr, $emu:expr, $mem:ident, $addr_offset:ident, $read:block, $read_wifi:block) => {{
        let $addr_offset = $addr & 0x00FFFFFF;
        let $mem = get_mem_mut!($emu);
        match $cpu {
            ARM9 => $read,
            ARM7 => {
                if unlikely($addr_offset >= 0x800000) {
                    let $addr_offset = $addr_offset & !0x8000;
                    if unlikely((0x804000..0x806000).contains(&$addr_offset)) {
                        $read_wifi
                    } else {
                        $read
                    }
                } else {
                    $read
                }
            }
        }
    }};
}

macro_rules! write_dtcm {
    ($cpu:expr, $tcm:expr, $addr:expr, $emu:expr, $mem:ident, $shm_offset:ident, $write:block) => {{
        if $cpu == ARM9 && $tcm {
            let cp15 = get_cp15!($emu);
            if unlikely($addr >= cp15.dtcm_addr && $addr < cp15.dtcm_addr + cp15.dtcm_size && cp15.dtcm_state != TcmState::Disabled) {
                let dtcm_addr = $addr - cp15.dtcm_addr;
                let $shm_offset = regions::DTCM_REGION.shm_offset as u32 + (dtcm_addr & (regions::DTCM_SIZE - 1));
                let $mem = get_mem_mut!($emu);
                $write;
                return;
            }
        }
    }};
}

macro_rules! write_itcm {
    ($cpu:expr, $tcm:expr, $addr:expr, $size:expr, $emu:expr, $mem:ident, $shm_offset:ident, $write:block) => {{
        if $cpu == ARM9 && $tcm {
            let cp15 = get_cp15!($emu);
            if $addr < cp15.itcm_size && cp15.itcm_state != TcmState::Disabled {
                let $shm_offset = regions::ITCM_REGION.shm_offset as u32 + ($addr & (regions::ITCM_SIZE - 1));
                let $mem = get_mem_mut!($emu);
                $write;
                get_jit_mut!($emu).invalidate_block($addr, $size);
            }
        }
    }};
}

macro_rules! write_main {
    ($addr:expr, $size:expr, $emu:expr, $mem:ident, $shm_offset:ident, $write:block) => {{
        let $shm_offset = regions::MAIN_REGION.shm_offset as u32 + ($addr & (regions::MAIN_SIZE - 1));
        let $mem = get_mem_mut!($emu);
        $write;
        get_jit_mut!($emu).invalidate_block($addr, $size);
    }};
}

macro_rules! write_wram {
    ($cpu:expr, $addr:expr, $size:expr, $emu:expr, $mem:ident, $shm_offset:ident, $write:block) => {{
        let $mem = get_mem_mut!($emu);
        let $shm_offset = $mem.wram.get_shm_offset::<{ $cpu }>($addr) as u32;
        $write;
        if $cpu == ARM7 {
            get_jit_mut!($emu).invalidate_block($addr, $size);
        }
    }};
}

macro_rules! write_io_ports {
    ($cpu:expr, $addr:expr, $emu:expr, $mem:ident, $addr_offset:ident, $write:block, $write_wifi:block) => {{
        let $addr_offset = $addr & 0x00FFFFFF;
        let $mem = get_mem_mut!($emu);
        match CPU {
            ARM9 => $write,
            ARM7 => {
                if unlikely($addr_offset >= 0x800000) {
                    let $addr_offset = $addr_offset & !0x8000;
                    if unlikely((0x804000..0x806000).contains(&$addr_offset)) {
                        $write_wifi;
                    } else {
                        $write;
                    }
                } else {
                    $write;
                }
            }
        }
    }};
}

macro_rules! write_vram {
    ($addr:expr, $size:expr, $emu:expr, $mem:ident, $write:block) => {
        let $mem = get_mem_mut!($emu);
        $write;
        get_jit_mut!($emu).invalidate_block($addr, $size);
    };
}

struct MemoryIo<const CPU: CpuType, const TCM: bool, T: Convert> {
    _data: PhantomData<T>,
}

impl<const CPU: CpuType, const TCM: bool, T: Convert> MemoryIo<CPU, TCM, T> {
    const READ_LUT: [fn(u32, &mut Emu) -> T; 16] = create_io_read_lut!();
    const WRITE_LUT: [fn(u32, T, &mut Emu); 9] = create_io_write_lut!();

    fn read(addr: u32, emu: &mut Emu) -> T {
        read_dtcm!(CPU, TCM, addr, emu, mem, shm_offset, { utils::read_from_mem(&mem.shm, shm_offset) });
        Self::READ_LUT[((addr >> 24) & 0xF) as usize](addr, emu)
    }

    fn read_itcm(addr: u32, emu: &mut Emu) -> T {
        read_itcm!(CPU, TCM, addr, emu, mem, shm_offset, { utils::read_from_mem(&mem.shm, shm_offset) }, { T::from(0) })
    }

    fn read_main(addr: u32, emu: &mut Emu) -> T {
        read_main!(addr, emu, mem, shm_offset, { utils::read_from_mem(&mem.shm, shm_offset) })
    }

    fn read_wram(addr: u32, emu: &mut Emu) -> T {
        read_wram!(CPU, addr, emu, mem, shm_offset, { utils::read_from_mem(&mem.shm, shm_offset) })
    }

    fn read_io_ports(addr: u32, emu: &mut Emu) -> T {
        read_io_ports!(
            CPU,
            addr,
            emu,
            mem,
            addr_offset,
            {
                match CPU {
                    ARM9 => mem.io_arm9.read(addr_offset, emu),
                    ARM7 => mem.io_arm7.read(addr_offset, emu),
                }
            },
            { mem.wifi.read(addr_offset) }
        )
    }

    fn read_palettes(addr: u32, emu: &mut Emu) -> T {
        get_mem!(emu).palettes.read(addr)
    }

    fn read_vram(addr: u32, emu: &mut Emu) -> T {
        get_mem!(emu).vram.read::<CPU, _>(addr)
    }

    fn read_oam(addr: u32, emu: &mut Emu) -> T {
        get_mem!(emu).oam.read(addr)
    }

    fn read_gba(_: u32, _: &mut Emu) -> T {
        T::from(0xFFFFFFFF)
    }

    fn read_invalid(_: u32, _: &mut Emu) -> T {
        unsafe { unreachable_unchecked() }
    }

    fn read_bios(_: u32, _: &mut Emu) -> T {
        match CPU {
            ARM9 => T::from(0),
            ARM7 => unsafe { unreachable_unchecked() },
        }
    }

    fn write(addr: u32, value: T, emu: &mut Emu) {
        write_dtcm!(CPU, TCM, addr, emu, mem, shm_offset, { utils::write_to_mem(&mut mem.shm, shm_offset, value) });
        let func = unsafe { Self::WRITE_LUT.get_unchecked(((addr >> 24) & 0xF) as usize) };
        func(addr, value, emu);
    }

    fn write_itcm(addr: u32, value: T, emu: &mut Emu) {
        write_itcm!(CPU, TCM, addr, size_of::<T>(), emu, mem, shm_offset, { utils::write_to_mem(&mut mem.shm, shm_offset, value) });
    }

    fn write_main(addr: u32, value: T, emu: &mut Emu) {
        write_main!(addr, size_of::<T>(), emu, mem, shm_offset, { utils::write_to_mem(&mut mem.shm, shm_offset, value) });
    }

    fn write_wram(addr: u32, value: T, emu: &mut Emu) {
        write_wram!(CPU, addr, size_of::<T>(), emu, mem, shm_offset, { utils::write_to_mem(&mut mem.shm, shm_offset, value) });
    }

    fn write_io_ports(addr: u32, value: T, emu: &mut Emu) {
        write_io_ports!(
            CPU,
            addr,
            emu,
            mem,
            addr_offset,
            {
                match CPU {
                    ARM9 => mem.io_arm9.write(addr_offset, value, emu),
                    ARM7 => mem.io_arm7.write(addr_offset, value, emu),
                }
            },
            { mem.wifi.write(addr_offset, value) }
        );
    }

    fn write_palettes(addr: u32, value: T, emu: &mut Emu) {
        get_mem_mut!(emu).palettes.write(addr, value);
    }

    fn write_vram(addr: u32, value: T, emu: &mut Emu) {
        write_vram!(addr, size_of::<T>(), emu, mem, { mem.vram.write::<CPU, _>(addr, value) });
    }

    fn write_oam(addr: u32, value: T, emu: &mut Emu) {
        get_mem_mut!(emu).oam.write(addr, value);
    }

    fn write_gba(_: u32, _: T, _: &mut Emu) {}
}

struct MemoryReadMultipleIo<const CPU: CpuType, T: Convert, F: FnMut(T)> {
    _data: PhantomData<T>,
    _data1: PhantomData<F>,
}

impl<const CPU: CpuType, T: Convert, F: FnMut(T)> MemoryReadMultipleIo<CPU, T, F> {
    const READ_LUT: [fn(u32, usize, F, &mut Emu); 16] = create_io_read_lut!();

    fn read(addr: u32, size: usize, mut write_value: F, emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        read_dtcm!(CPU, true, addr, emu, mem, shm_offset, {
            for i in 0..size {
                write_value(utils::read_from_mem(&mem.shm, shm_offset + (i << read_shift) as u32));
            }
        });
        Self::READ_LUT[((addr >> 24) & 0xF) as usize](addr, size, write_value, emu)
    }

    fn read_itcm(addr: u32, size: usize, mut write_value: F, emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        read_itcm!(
            CPU,
            true,
            addr,
            emu,
            mem,
            shm_offset,
            {
                for i in 0..size {
                    write_value(utils::read_from_mem(&mem.shm, shm_offset + (i << read_shift) as u32));
                }
            },
            {
                for _ in 0..size {
                    write_value(T::from(0));
                }
            }
        );
    }

    fn read_main(addr: u32, size: usize, mut write_value: F, emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        read_main!(addr, emu, mem, shm_offset, {
            for i in 0..size {
                write_value(utils::read_from_mem(&mem.shm, shm_offset + (i << read_shift) as u32));
            }
        });
    }

    fn read_wram(addr: u32, size: usize, mut write_value: F, emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        read_wram!(CPU, addr, emu, mem, shm_offset, {
            for i in 0..size {
                write_value(utils::read_from_mem(&mem.shm, shm_offset + (i << read_shift) as u32));
            }
        });
    }

    fn read_io_ports(addr: u32, size: usize, mut write_value: F, emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        read_io_ports!(
            CPU,
            addr,
            emu,
            mem,
            addr_offset,
            {
                for i in 0..size {
                    match CPU {
                        ARM9 => write_value(mem.io_arm9.read(addr_offset + (i << read_shift) as u32, emu)),
                        ARM7 => write_value(mem.io_arm7.read(addr_offset + (i << read_shift) as u32, emu)),
                    }
                }
            },
            {
                for i in 0..size {
                    write_value(mem.wifi.read(addr_offset + (i << read_shift) as u32));
                }
            }
        );
    }

    fn read_palettes(addr: u32, size: usize, mut write_value: F, emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        let mem = get_mem!(emu);
        for i in 0..size {
            write_value(mem.palettes.read(addr + (i << read_shift) as u32));
        }
    }

    fn read_vram(addr: u32, size: usize, mut write_value: F, emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        let mem = get_mem!(emu);
        for i in 0..size {
            write_value(mem.vram.read::<CPU, _>(addr + (i << read_shift) as u32));
        }
    }

    fn read_oam(addr: u32, size: usize, mut write_value: F, emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        let mem = get_mem!(emu);
        for i in 0..size {
            write_value(mem.oam.read(addr + (i << read_shift) as u32));
        }
    }

    fn read_gba(_: u32, size: usize, mut write_value: F, _: &mut Emu) {
        for _ in 0..size {
            write_value(T::from(0xFFFFFFFF));
        }
    }

    fn read_invalid(_: u32, _: usize, _: F, _: &mut Emu) {
        unsafe { unreachable_unchecked() }
    }

    fn read_bios(_: u32, size: usize, mut write_value: F, _: &mut Emu) {
        for _ in 0..size {
            write_value(T::from(0));
        }
    }
}

struct MemoryWriteMultipleIo<const CPU: CpuType, T: Convert, F: FnMut() -> T> {
    _data: PhantomData<T>,
    _data1: PhantomData<F>,
}

impl<const CPU: CpuType, T: Convert, F: FnMut() -> T> MemoryWriteMultipleIo<CPU, T, F> {
    const WRITE_LUT: [fn(u32, usize, F, &mut Emu); 9] = create_io_write_lut!();

    fn write(addr: u32, size: usize, mut read_value: F, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_dtcm!(CPU, true, addr, emu, mem, shm_offset, {
            for i in 0..size {
                utils::write_to_mem(&mut mem.shm, shm_offset + (i << write_shift) as u32, read_value())
            }
        });
        let func = unsafe { Self::WRITE_LUT.get_unchecked(((addr >> 24) & 0xF) as usize) };
        func(addr, size, read_value, emu);
    }

    fn write_itcm(addr: u32, size: usize, mut read_value: F, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_itcm!(CPU, true, addr, size << write_shift, emu, mem, shm_offset, {
            for i in 0..size {
                utils::write_to_mem(&mut mem.shm, shm_offset + (i << write_shift) as u32, read_value());
            }
        });
    }

    fn write_main(addr: u32, size: usize, mut read_value: F, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_main!(addr, size << write_shift, emu, mem, shm_offset, {
            for i in 0..size {
                utils::write_to_mem(&mut mem.shm, shm_offset + (i << write_shift) as u32, read_value());
            }
        });
    }

    fn write_wram(addr: u32, size: usize, mut read_value: F, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_wram!(CPU, addr, size << write_shift, emu, mem, shm_offset, {
            for i in 0..size {
                utils::write_to_mem(&mut mem.shm, shm_offset + (i << write_shift) as u32, read_value());
            }
        });
    }

    fn write_io_ports(addr: u32, size: usize, mut read_value: F, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_io_ports!(
            CPU,
            addr,
            emu,
            mem,
            addr_offset,
            {
                for i in 0..size {
                    match CPU {
                        ARM9 => mem.io_arm9.write(addr_offset + (i << write_shift) as u32, read_value(), emu),
                        ARM7 => mem.io_arm7.write(addr_offset + (i << write_shift) as u32, read_value(), emu),
                    }
                }
            },
            {
                for i in 0..size {
                    mem.wifi.write(addr_offset + (i << write_shift) as u32, read_value());
                }
            }
        )
    }

    fn write_palettes(addr: u32, size: usize, mut read_value: F, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        let mem = get_mem_mut!(emu);
        for i in 0..size {
            mem.palettes.write(addr + (i << write_shift) as u32, read_value());
        }
    }

    fn write_vram(addr: u32, size: usize, mut read_value: F, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        let mem = get_mem_mut!(emu);
        for i in 0..size {
            mem.vram.write::<CPU, _>(addr + (i << write_shift) as u32, read_value());
        }
        mem.jit.invalidate_block(addr, size << write_shift);
    }

    fn write_oam(addr: u32, size: usize, mut read_value: F, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        let mem = get_mem_mut!(emu);
        for i in 0..size {
            mem.oam.write(addr + (i << write_shift) as u32, read_value());
        }
    }

    fn write_gba(_: u32, _: usize, _: F, _: &mut Emu) {}
}

struct MemoryMultipleSliceIo<const CPU: CpuType, const TCM: bool, T: Convert> {
    _data: PhantomData<T>,
}

impl<const CPU: CpuType, const TCM: bool, T: Convert> MemoryMultipleSliceIo<CPU, TCM, T> {
    const READ_LUT: [fn(u32, &mut [T], &mut Emu); 16] = create_io_read_lut!();
    const WRITE_LUT: [fn(u32, &[T], &mut Emu); 9] = create_io_write_lut!();

    fn read(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_dtcm!(CPU, TCM, addr, emu, mem, shm_offset, {
            utils::read_from_mem_slice(&mem.shm, shm_offset, slice);
        });
        Self::READ_LUT[((addr >> 24) & 0xF) as usize](addr, slice, emu);
    }

    fn read_itcm(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_itcm!(
            CPU,
            TCM,
            addr,
            emu,
            mem,
            shm_offset,
            {
                utils::read_from_mem_slice(&mem.shm, shm_offset, slice);
            },
            {
                slice.fill(T::from(0));
            }
        );
    }

    fn read_main(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_main!(addr, emu, mem, shm_offset, {
            utils::read_from_mem_slice(&mem.shm, shm_offset, slice);
        });
    }

    fn read_wram(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_wram!(CPU, addr, emu, mem, shm_offset, {
            utils::read_from_mem_slice(&mem.shm, shm_offset, slice);
        });
    }

    fn read_io_ports(addr: u32, slice: &mut [T], emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        read_io_ports!(
            CPU,
            addr,
            emu,
            mem,
            addr_offset,
            {
                for i in 0..slice.len() {
                    slice[i] = match CPU {
                        ARM9 => mem.io_arm9.read(addr_offset + (i << read_shift) as u32, emu),
                        ARM7 => mem.io_arm7.read(addr_offset + (i << read_shift) as u32, emu),
                    };
                }
            },
            {
                mem.wifi.read_slice(addr_offset, slice);
            }
        );
    }

    fn read_palettes(addr: u32, slice: &mut [T], emu: &mut Emu) {
        get_mem!(emu).palettes.read_slice(addr, slice);
    }

    fn read_vram(addr: u32, slice: &mut [T], emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        let mem = get_mem!(emu);
        for i in 0..slice.len() {
            slice[i] = mem.vram.read::<CPU, _>(addr + (i << read_shift) as u32);
        }
    }

    fn read_oam(addr: u32, slice: &mut [T], emu: &mut Emu) {
        get_mem!(emu).oam.read_slice(addr, slice);
    }

    fn read_gba(_: u32, slice: &mut [T], _: &mut Emu) {
        slice.fill(T::from(0xFFFFFFFF));
    }

    fn read_invalid(_: u32, _: &mut [T], _: &mut Emu) {
        unsafe { unreachable_unchecked() }
    }

    fn read_bios(_: u32, slice: &mut [T], _: &mut Emu) {
        slice.fill(T::from(0));
    }

    fn write(addr: u32, slice: &[T], emu: &mut Emu) {
        write_dtcm!(CPU, TCM, addr, emu, mem, shm_offset, {
            utils::write_to_mem_slice(&mut mem.shm, shm_offset as usize, slice);
        });
        let func = unsafe { Self::WRITE_LUT.get_unchecked(((addr >> 24) & 0xF) as usize) };
        func(addr, slice, emu);
    }

    fn write_itcm(addr: u32, slice: &[T], emu: &mut Emu) {
        write_itcm!(CPU, TCM, addr, size_of_val(slice), emu, mem, shm_offset, {
            utils::write_to_mem_slice(&mut mem.shm, shm_offset as usize, slice);
        });
    }

    fn write_main(addr: u32, slice: &[T], emu: &mut Emu) {
        write_main!(addr, size_of_val(slice), emu, mem, shm_offset, {
            utils::write_to_mem_slice(&mut mem.shm, shm_offset as usize, slice);
        });
    }

    fn write_wram(addr: u32, slice: &[T], emu: &mut Emu) {
        write_wram!(CPU, addr, size_of_val(slice), emu, mem, shm_offset, {
            utils::write_to_mem_slice(&mut mem.shm, shm_offset as usize, slice);
        });
    }

    fn write_io_ports(addr: u32, slice: &[T], emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_io_ports!(
            CPU,
            addr,
            emu,
            mem,
            addr_offset,
            {
                for i in 0..slice.len() {
                    match CPU {
                        ARM9 => mem.io_arm9.write(addr_offset + (i << write_shift) as u32, slice[i], emu),
                        ARM7 => mem.io_arm7.write(addr_offset + (i << write_shift) as u32, slice[i], emu),
                    }
                }
            },
            {
                mem.wifi.write_slice(addr_offset, slice);
            }
        )
    }

    fn write_palettes(addr: u32, slice: &[T], emu: &mut Emu) {
        get_mem_mut!(emu).palettes.write_slice(addr, slice);
    }

    fn write_vram(addr: u32, slice: &[T], emu: &mut Emu) {
        let mem = get_mem_mut!(emu);
        mem.vram.write_slice::<CPU, _>(addr, slice);
        mem.jit.invalidate_block(addr, size_of_val(slice));
    }

    fn write_oam(addr: u32, slice: &[T], emu: &mut Emu) {
        get_mem_mut!(emu).oam.write_slice(addr, slice);
    }

    fn write_gba(_: u32, _: &[T], _: &mut Emu) {}
}

struct MemoryFixedSliceIo<const CPU: CpuType, const TCM: bool, T: Convert> {
    _data: PhantomData<T>,
}

impl<const CPU: CpuType, const TCM: bool, T: Convert> MemoryFixedSliceIo<CPU, TCM, T> {
    const READ_LUT: [fn(u32, &mut [T], &mut Emu); 16] = create_io_read_lut!();
    const WRITE_LUT: [fn(u32, &[T], &mut Emu); 9] = create_io_write_lut!();

    fn read(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_dtcm!(CPU, TCM, addr, emu, mem, shm_offset, {
            slice.fill(utils::read_from_mem(&mem.shm, shm_offset));
        });
        Self::READ_LUT[((addr >> 24) & 0xF) as usize](addr, slice, emu);
    }

    fn read_itcm(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_itcm!(
            CPU,
            TCM,
            addr,
            emu,
            mem,
            shm_offset,
            {
                slice.fill(utils::read_from_mem(&mem.shm, shm_offset));
            },
            {
                slice.fill(T::from(0));
            }
        )
    }

    fn read_main(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_main!(addr, emu, mem, shm_offset, {
            slice.fill(utils::read_from_mem(&mem.shm, shm_offset));
        })
    }

    fn read_wram(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_wram!(CPU, addr, emu, mem, shm_offset, {
            slice.fill(utils::read_from_mem(&mem.shm, shm_offset));
        });
    }

    fn read_io_ports(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_io_ports!(
            CPU,
            addr,
            emu,
            mem,
            addr_offset,
            {
                for i in 0..slice.len() {
                    slice[i] = match CPU {
                        ARM9 => mem.io_arm9.read(addr_offset, emu),
                        ARM7 => mem.io_arm7.read(addr_offset, emu),
                    };
                }
            },
            {
                slice.fill(mem.wifi.read(addr_offset));
            }
        )
    }

    fn read_palettes(addr: u32, slice: &mut [T], emu: &mut Emu) {
        slice.fill(get_mem!(emu).palettes.read(addr));
    }

    fn read_vram(addr: u32, slice: &mut [T], emu: &mut Emu) {
        slice.fill(get_mem!(emu).vram.read::<CPU, _>(addr));
    }

    fn read_oam(addr: u32, slice: &mut [T], emu: &mut Emu) {
        slice.fill(get_mem!(emu).oam.read(addr));
    }

    fn read_gba(_: u32, slice: &mut [T], _: &mut Emu) {
        slice.fill(T::from(0xFFFFFFFF));
    }

    fn read_invalid(_: u32, _: &mut [T], _: &mut Emu) {
        unsafe { unreachable_unchecked() }
    }

    fn read_bios(_: u32, slice: &mut [T], _: &mut Emu) {
        slice.fill(T::from(0));
    }

    fn write(addr: u32, slice: &[T], emu: &mut Emu) {
        write_dtcm!(CPU, TCM, addr, emu, mem, shm_offset, {
            utils::write_to_mem(&mut mem.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() })
        });
        let func = unsafe { Self::WRITE_LUT.get_unchecked(((addr >> 24) & 0xF) as usize) };
        func(addr, slice, emu);
    }

    fn write_itcm(addr: u32, slice: &[T], emu: &mut Emu) {
        write_itcm!(CPU, TCM, addr, size_of_val(slice), emu, mem, shm_offset, {
            utils::write_to_mem(&mut mem.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() });
        });
    }

    fn write_main(addr: u32, slice: &[T], emu: &mut Emu) {
        write_main!(addr, size_of_val(slice), emu, mem, shm_offset, {
            utils::write_to_mem(&mut mem.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() });
        });
    }

    fn write_wram(addr: u32, slice: &[T], emu: &mut Emu) {
        write_wram!(CPU, addr, size_of_val(slice), emu, mem, shm_offset, {
            utils::write_to_mem(&mut mem.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() });
        })
    }

    fn write_io_ports(addr: u32, slice: &[T], emu: &mut Emu) {
        write_io_ports!(
            CPU,
            addr,
            emu,
            mem,
            addr_offset,
            {
                match CPU {
                    ARM9 => mem.io_arm9.write_fixed_slice(addr_offset, slice, emu),
                    ARM7 => mem.io_arm7.write_fixed_slice(addr_offset, slice, emu),
                }
            },
            { mem.wifi.write(addr_offset, unsafe { *slice.last().unwrap_unchecked() }) }
        );
    }

    fn write_palettes(addr: u32, slice: &[T], emu: &mut Emu) {
        get_mem_mut!(emu).palettes.write(addr, unsafe { *slice.last().unwrap_unchecked() });
    }

    fn write_vram(addr: u32, slice: &[T], emu: &mut Emu) {
        let emu = get_mem_mut!(emu);
        for i in 0..slice.len() {
            emu.vram.write::<CPU, _>(addr, slice[i]);
        }
    }

    fn write_oam(addr: u32, slice: &[T], emu: &mut Emu) {
        get_mem_mut!(emu).oam.write(addr, unsafe { *slice.last().unwrap_unchecked() })
    }

    fn write_gba(_: u32, _: &[T], _: &mut Emu) {}
}

struct MemoryMultipleMemsetIo<const CPU: CpuType, const TCM: bool, T: Convert> {
    _data: PhantomData<T>,
}

impl<const CPU: CpuType, const TCM: bool, T: Convert> MemoryMultipleMemsetIo<CPU, TCM, T> {
    const WRITE_LUT: [fn(u32, T, usize, &mut Emu); 9] = create_io_write_lut!();

    fn write(addr: u32, value: T, size: usize, emu: &mut Emu) {
        write_dtcm!(CPU, TCM, addr, emu, mem, shm_offset, {
            utils::write_memset(&mut mem.shm, shm_offset as usize, value, size);
        });
        let func = unsafe { Self::WRITE_LUT.get_unchecked(((addr >> 24) & 0xF) as usize) };
        func(addr, value, size, emu);
    }

    fn write_itcm(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_itcm!(CPU, TCM, addr, size << write_shift, emu, mem, shm_offset, {
            utils::write_memset(&mut mem.shm, shm_offset as usize, value, size);
        });
    }

    fn write_main(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_main!(addr, size << write_shift, emu, mem, shm_offset, {
            utils::write_memset(&mut mem.shm, shm_offset as usize, value, size);
        });
    }

    fn write_wram(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_wram!(CPU, addr, size << write_shift, emu, mem, shm_offset, {
            utils::write_memset(&mut mem.shm, shm_offset as usize, value, size);
        });
    }

    fn write_io_ports(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_io_ports!(
            CPU,
            addr,
            emu,
            mem,
            addr_offset,
            {
                for i in 0..size {
                    match CPU {
                        ARM9 => mem.io_arm9.write(addr_offset + (i << write_shift) as u32, value, emu),
                        ARM7 => mem.io_arm7.write(addr_offset + (i << write_shift) as u32, value, emu),
                    }
                }
            },
            {
                mem.wifi.write_memset(addr_offset, value, size);
            }
        )
    }

    fn write_palettes(addr: u32, value: T, size: usize, emu: &mut Emu) {
        get_mem_mut!(emu).palettes.write_memset(addr, value, size);
    }

    fn write_vram(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        let mem = get_mem_mut!(emu);
        for i in 0..size {
            mem.vram.write::<CPU, _>(addr + (i << write_shift) as u32, value);
        }
    }

    fn write_oam(addr: u32, value: T, size: usize, emu: &mut Emu) {
        get_mem_mut!(emu).oam.write_memset(addr, value, size);
    }

    fn write_gba(_: u32, _: T, _: usize, _: &mut Emu) {}
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
            vram: Vram::default(),
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
        debug_println!("{CPU:?} memory read at {addr:x}");
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, false>(aligned_addr) as u32;
        if shm_offset != 0 {
            let ret: T = utils::read_from_mem(&self.shm, shm_offset);
            debug_println!("{CPU:?} memory read at {addr:x} with value {:x}", ret.into());
            return ret;
        }

        let ret: T = MemoryIo::<CPU, TCM, T>::read(aligned_addr, emu);
        debug_println!("{CPU:?} memory read at {addr:x} with value {:x}", ret.into());
        ret
    }

    pub fn read_multiple<const CPU: CpuType, T: Convert, F: FnMut(T)>(&mut self, addr: u32, emu: &mut Emu, size: usize, mut write_value: F) {
        debug_println!("{CPU:?} multiple memory read at {addr:x} with size {size}");
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, true, false>(aligned_addr) as u32;
        if shm_offset != 0 {
            let read_shift = size_of::<T>() >> 1;
            for i in 0..size {
                write_value(utils::read_from_mem(&self.shm, shm_offset + (i << read_shift) as u32));
            }
            return;
        }

        MemoryReadMultipleIo::<CPU, T, F>::read(aligned_addr, size, write_value, emu);
    }

    pub fn read_multiple_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, emu: &mut Emu, slice: &mut [T]) {
        debug_println!("{CPU:?} slice memory read at {addr:x} with size {}", size_of_val(slice));
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, false>(aligned_addr) as u32;
        if shm_offset != 0 {
            utils::read_from_mem_slice(&self.shm, shm_offset, slice);
            return;
        }

        MemoryMultipleSliceIo::<CPU, TCM, T>::read(aligned_addr, slice, emu);
    }

    pub fn read_fixed_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, emu: &mut Emu, slice: &mut [T]) {
        debug_println!("{CPU:?} fixed slice memory read at {addr:x} with size {}", size_of_val(slice));
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, false>(aligned_addr) as u32;
        if shm_offset != 0 {
            slice.fill(utils::read_from_mem(&self.shm, shm_offset));
            return;
        }

        MemoryFixedSliceIo::<CPU, TCM, T>::read(aligned_addr, slice, emu);
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

        MemoryIo::<CPU, TCM, T>::write(aligned_addr, value, emu);
    }

    pub fn write_multiple<const CPU: CpuType, T: Convert, F: FnMut() -> T>(&mut self, addr: u32, emu: &mut Emu, size: usize, mut read_value: F) {
        debug_println!("{CPU:?} multiple memory write at {addr:x} with size {size}");
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, true, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            let write_shift = size_of::<T>() >> 1;
            for i in 0..size {
                utils::write_to_mem(&mut self.shm, shm_offset + (i << write_shift) as u32, read_value());
            }
            return;
        }

        MemoryWriteMultipleIo::<CPU, T, F>::write(aligned_addr, size, read_value, emu);
    }

    pub fn write_multiple_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, emu: &mut Emu, slice: &[T]) {
        debug_println!("{CPU:?} fixed slice memory write at {addr:x} with size {}", slice.len());
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            utils::write_to_mem_slice(&mut self.shm, shm_offset as usize, slice);
            return;
        }

        MemoryMultipleSliceIo::<CPU, TCM, T>::write(aligned_addr, slice, emu);
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

        MemoryFixedSliceIo::<CPU, TCM, T>::write(aligned_addr, slice, emu);
    }

    pub fn write_multiple_memset<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, value: T, size: usize, emu: &mut Emu) {
        debug_println!("{CPU:?} multiple memset memory write at {addr:x} with size {size}");
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            utils::write_memset(&mut self.shm, shm_offset as usize, value, size);
            return;
        }

        MemoryMultipleMemsetIo::<CPU, TCM, T>::write(aligned_addr, value, size, emu);
    }
}

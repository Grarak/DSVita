use crate::core::cp15::TcmState;
use crate::core::emu::Emu;
use crate::core::memory::mmu::{MmuArm7, MmuArm9, MMU_PAGE_SHIFT, MMU_PAGE_SIZE};
use crate::core::memory::regions;
use crate::core::memory::regions::{OAM_SIZE, STANDARD_PALETTES_SIZE};
use crate::core::memory::vram::Vram;
use crate::core::memory::wifi::Wifi;
use crate::core::memory::wram::Wram;
use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::logging::debug_println;
use crate::mmap::Shm;
use crate::utils::Convert;
use crate::{utils, DEBUG_LOG};
use std::hint::unreachable_unchecked;
use std::intrinsics::unlikely;
use std::marker::PhantomData;
use std::mem;
use CpuType::ARM7;

pub struct Memory {
    pub shm: Shm,
    pub wram: Wram,
    pub wifi: Wifi,
    pub vram: Vram,
    pub mmu_arm9: MmuArm9,
    pub mmu_arm7: MmuArm7,
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
        ]
    };
}

macro_rules! read_dtcm {
    ($cpu:expr, $tcm:expr, $addr:expr, $emu:expr, $shm_offset:ident, $read:block) => {{
        if $cpu == ARM9 && $tcm {
            if $addr >= $emu.cp15.dtcm_addr && $addr < $emu.cp15.dtcm_addr + $emu.cp15.dtcm_size && $emu.cp15.dtcm_state == TcmState::RW {
                let dtcm_addr = $addr - $emu.cp15.dtcm_addr;
                let $shm_offset = regions::DTCM_REGION.shm_offset as u32 + (dtcm_addr & (regions::DTCM_SIZE - 1));
                return $read;
            }
        }
    }};
}

macro_rules! read_itcm {
    ($cpu:expr, $tcm:expr, $addr:expr, $emu:expr, $shm_offset:ident, $read:block, $read_zero:block) => {{
        match $cpu {
            ARM9 => {
                if $tcm {
                    if $addr < $emu.cp15.itcm_size && $emu.cp15.itcm_state == TcmState::RW {
                        debug_println!("{:?} itcm read at {:x}", $cpu, $addr);
                        let $shm_offset = regions::ITCM_REGION.shm_offset as u32 + ($addr & (regions::ITCM_SIZE - 1));
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
    ($addr:expr, $emu:expr, $shm_offset:ident, $read:block) => {{
        let $shm_offset = regions::MAIN_REGION.shm_offset as u32 + ($addr & (regions::MAIN_SIZE - 1));
        $read
    }};
}

macro_rules! read_wram {
    ($cpu:expr, $addr:expr, $emu:expr, $shm_offset:ident, $read:block) => {{
        let $shm_offset = $emu.mem.wram.get_shm_offset::<{ $cpu }>($addr) as u32;
        $read
    }};
}

macro_rules! read_io_ports {
    ($cpu:expr, $addr:expr, $addr_offset:ident, $read:block, $read_wifi:block) => {{
        let $addr_offset = $addr & 0x00FFFFFF;
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
    ($cpu:expr, $tcm:expr, $addr:expr, $emu:expr, $shm_offset:ident, $write:block) => {{
        if $cpu == ARM9 && $tcm {
            if $addr >= $emu.cp15.dtcm_addr && $addr < $emu.cp15.dtcm_addr + $emu.cp15.dtcm_size && $emu.cp15.dtcm_state != TcmState::Disabled {
                let dtcm_addr = $addr - $emu.cp15.dtcm_addr;
                let $shm_offset = regions::DTCM_REGION.shm_offset as u32 + (dtcm_addr & (regions::DTCM_SIZE - 1));
                $write;
                return;
            }
        }
    }};
}

macro_rules! write_itcm {
    ($cpu:expr, $tcm:expr, $addr:expr, $size:expr, $emu:expr, $shm_offset:ident, $write:block) => {{
        if $cpu == ARM9 && $tcm {
            if $addr < $emu.cp15.itcm_size && $emu.cp15.itcm_state != TcmState::Disabled {
                let $shm_offset = regions::ITCM_REGION.shm_offset as u32 + ($addr & (regions::ITCM_SIZE - 1));
                $write;
                $emu.jit.invalidate_block($addr, $size);
            }
        }
    }};
}

macro_rules! write_main {
    ($addr:expr, $size:expr, $emu:expr, $shm_offset:ident, $write:block) => {{
        let $shm_offset = regions::MAIN_REGION.shm_offset as u32 + ($addr & (regions::MAIN_SIZE - 1));
        $write;
        $emu.jit.invalidate_block($addr, $size);
    }};
}

macro_rules! write_wram {
    ($cpu:expr, $addr:expr, $size:expr, $emu:expr, $shm_offset:ident, $write:block) => {{
        let $shm_offset = $emu.mem.wram.get_shm_offset::<{ $cpu }>($addr) as u32;
        $write;
        if $cpu == ARM7 {
            $emu.jit.invalidate_block($addr, $size);
        }
    }};
}

macro_rules! write_io_ports {
    ($cpu:expr, $addr:expr, $addr_offset:ident, $write:block, $write_wifi:block) => {{
        let $addr_offset = $addr & 0x00FFFFFF;
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
    ($addr:expr, $size:expr, $emu:expr, $write:block) => {
        $write;
        $emu.jit.invalidate_block($addr, $size);
    };
}

struct MemoryIo<const CPU: CpuType, const TCM: bool, T: Convert> {
    _data: PhantomData<T>,
}

impl<const CPU: CpuType, const TCM: bool, T: Convert> MemoryIo<CPU, TCM, T> {
    const READ_LUT: [fn(u32, &mut Emu) -> T; 7] = create_io_read_lut!();
    const WRITE_LUT: [fn(u32, T, &mut Emu); 7] = create_io_write_lut!();

    fn read(addr: u32, emu: &mut Emu) -> T {
        read_dtcm!(CPU, TCM, addr, emu, shm_offset, { utils::read_from_mem(&emu.mem.shm, shm_offset) });
        unsafe { Self::READ_LUT.get_unchecked(((addr >> 24) & 0xF) as usize)(addr, emu) }
    }

    fn read_itcm(addr: u32, emu: &mut Emu) -> T {
        read_itcm!(CPU, TCM, addr, emu, shm_offset, { utils::read_from_mem(&emu.mem.shm, shm_offset) }, { T::from(0) })
    }

    fn read_main(addr: u32, emu: &mut Emu) -> T {
        read_main!(addr, emu, shm_offset, { utils::read_from_mem(&emu.mem.shm, shm_offset) })
    }

    fn read_wram(addr: u32, emu: &mut Emu) -> T {
        read_wram!(CPU, addr, emu, shm_offset, { utils::read_from_mem(&emu.mem.shm, shm_offset) })
    }

    fn read_io_ports(addr: u32, emu: &mut Emu) -> T {
        read_io_ports!(
            CPU,
            addr,
            addr_offset,
            {
                match CPU {
                    ARM9 => emu.io_arm9_read(addr_offset),
                    ARM7 => emu.io_arm7_read(addr_offset),
                }
            },
            { emu.mem.wifi.read(addr_offset) }
        )
    }

    fn read_palettes(_: u32, _: &mut Emu) -> T {
        unsafe { unreachable_unchecked() }
    }

    fn read_vram(addr: u32, emu: &mut Emu) -> T {
        emu.vram_read::<CPU, _>(addr)
    }

    fn write(addr: u32, value: T, emu: &mut Emu) {
        write_dtcm!(CPU, TCM, addr, emu, shm_offset, { utils::write_to_mem(&mut emu.mem.shm, shm_offset, value) });
        let func = unsafe { Self::WRITE_LUT.get_unchecked(((addr >> 24) & 0xF) as usize) };
        func(addr, value, emu);
    }

    fn write_itcm(addr: u32, value: T, emu: &mut Emu) {
        write_itcm!(CPU, TCM, addr, size_of::<T>(), emu, shm_offset, { utils::write_to_mem(&mut emu.mem.shm, shm_offset, value) });
    }

    fn write_main(addr: u32, value: T, emu: &mut Emu) {
        write_main!(addr, size_of::<T>(), emu, shm_offset, { utils::write_to_mem(&mut emu.mem.shm, shm_offset, value) });
    }

    fn write_wram(addr: u32, value: T, emu: &mut Emu) {
        write_wram!(CPU, addr, size_of::<T>(), emu, shm_offset, { utils::write_to_mem(&mut emu.mem.shm, shm_offset, value) });
    }

    fn write_io_ports(addr: u32, value: T, emu: &mut Emu) {
        write_io_ports!(
            CPU,
            addr,
            addr_offset,
            {
                match CPU {
                    ARM9 => emu.io_arm9_write(addr_offset, value),
                    ARM7 => emu.io_arm7_write(addr_offset, value),
                }
            },
            { emu.mem.wifi.write(addr_offset, value) }
        );
    }

    fn write_palettes(_: u32, _: T, _: &mut Emu) {
        unsafe { unreachable_unchecked() }
    }

    fn write_vram(addr: u32, value: T, emu: &mut Emu) {
        write_vram!(addr, size_of::<T>(), emu, { emu.vram_write::<CPU, _>(addr, value) });
    }
}

struct MemoryMultipleSliceIo<const CPU: CpuType, const TCM: bool, T: Convert> {
    _data: PhantomData<T>,
}

impl<const CPU: CpuType, const TCM: bool, T: Convert> MemoryMultipleSliceIo<CPU, TCM, T> {
    const READ_LUT: [fn(u32, &mut [T], &mut Emu); 7] = create_io_read_lut!();
    const WRITE_LUT: [fn(u32, &[T], &mut Emu); 7] = create_io_write_lut!();

    fn read(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_dtcm!(CPU, TCM, addr, emu, shm_offset, {
            utils::read_from_mem_slice(&emu.mem.shm, shm_offset, slice);
        });
        unsafe { Self::READ_LUT.get_unchecked(((addr >> 24) & 0xF) as usize)(addr, slice, emu) };
    }

    fn read_itcm(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_itcm!(
            CPU,
            TCM,
            addr,
            emu,
            shm_offset,
            {
                utils::read_from_mem_slice(&emu.mem.shm, shm_offset, slice);
            },
            {
                slice.fill(T::from(0));
            }
        );
    }

    fn read_main(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_main!(addr, emu, shm_offset, {
            utils::read_from_mem_slice(&emu.mem.shm, shm_offset, slice);
        });
    }

    fn read_wram(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_wram!(CPU, addr, emu, shm_offset, {
            utils::read_from_mem_slice(&emu.mem.shm, shm_offset, slice);
        });
    }

    fn read_io_ports(addr: u32, slice: &mut [T], emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        read_io_ports!(
            CPU,
            addr,
            addr_offset,
            {
                for i in 0..slice.len() {
                    slice[i] = match CPU {
                        ARM9 => emu.io_arm9_read(addr_offset + (i << read_shift) as u32),
                        ARM7 => emu.io_arm7_read(addr_offset + (i << read_shift) as u32),
                    };
                }
            },
            {
                emu.mem.wifi.read_slice(addr_offset, slice);
            }
        );
    }

    fn read_palettes(_: u32, _: &mut [T], _: &mut Emu) {
        unsafe { unreachable_unchecked() }
    }

    fn read_vram(addr: u32, slice: &mut [T], emu: &mut Emu) {
        let read_shift = size_of::<T>() >> 1;
        for i in 0..slice.len() {
            slice[i] = emu.vram_read::<CPU, _>(addr + (i << read_shift) as u32);
        }
    }

    fn write(addr: u32, slice: &[T], emu: &mut Emu) {
        write_dtcm!(CPU, TCM, addr, emu, shm_offset, {
            utils::write_to_mem_slice(&mut emu.mem.shm, shm_offset as usize, slice);
        });
        let func = unsafe { Self::WRITE_LUT.get_unchecked(((addr >> 24) & 0xF) as usize) };
        func(addr, slice, emu);
    }

    fn write_itcm(addr: u32, slice: &[T], emu: &mut Emu) {
        write_itcm!(CPU, TCM, addr, size_of_val(slice), emu, shm_offset, {
            utils::write_to_mem_slice(&mut emu.mem.shm, shm_offset as usize, slice);
        });
    }

    fn write_main(addr: u32, slice: &[T], emu: &mut Emu) {
        write_main!(addr, size_of_val(slice), emu, shm_offset, {
            utils::write_to_mem_slice(&mut emu.mem.shm, shm_offset as usize, slice);
        });
    }

    fn write_wram(addr: u32, slice: &[T], emu: &mut Emu) {
        write_wram!(CPU, addr, size_of_val(slice), emu, shm_offset, {
            utils::write_to_mem_slice(&mut emu.mem.shm, shm_offset as usize, slice);
        });
    }

    fn write_io_ports(addr: u32, slice: &[T], emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_io_ports!(
            CPU,
            addr,
            addr_offset,
            {
                for i in 0..slice.len() {
                    match CPU {
                        ARM9 => emu.io_arm9_write(addr_offset + (i << write_shift) as u32, slice[i]),
                        ARM7 => emu.io_arm7_write(addr_offset + (i << write_shift) as u32, slice[i]),
                    }
                }
            },
            {
                emu.mem.wifi.write_slice(addr_offset, slice);
            }
        )
    }

    fn write_palettes(_: u32, _: &[T], _: &mut Emu) {
        unsafe { unreachable_unchecked() }
    }

    fn write_vram(addr: u32, slice: &[T], emu: &mut Emu) {
        emu.vram_write_slice::<CPU, _>(addr, slice);
        emu.jit.invalidate_block(addr, size_of_val(slice));
    }
}

struct MemoryFixedSliceIo<const CPU: CpuType, const TCM: bool, T: Convert> {
    _data: PhantomData<T>,
}

impl<const CPU: CpuType, const TCM: bool, T: Convert> MemoryFixedSliceIo<CPU, TCM, T> {
    const READ_LUT: [fn(u32, &mut [T], &mut Emu); 7] = create_io_read_lut!();
    const WRITE_LUT: [fn(u32, &[T], &mut Emu); 7] = create_io_write_lut!();

    fn read(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_dtcm!(CPU, TCM, addr, emu, shm_offset, {
            slice.fill(utils::read_from_mem(&emu.mem.shm, shm_offset));
        });
        unsafe { Self::READ_LUT.get_unchecked(((addr >> 24) & 0xF) as usize)(addr, slice, emu) };
    }

    fn read_itcm(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_itcm!(
            CPU,
            TCM,
            addr,
            emu,
            shm_offset,
            {
                slice.fill(utils::read_from_mem(&emu.mem.shm, shm_offset));
            },
            {
                slice.fill(T::from(0));
            }
        )
    }

    fn read_main(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_main!(addr, emu, shm_offset, {
            slice.fill(utils::read_from_mem(&emu.mem.shm, shm_offset));
        })
    }

    fn read_wram(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_wram!(CPU, addr, emu, shm_offset, {
            slice.fill(utils::read_from_mem(&emu.mem.shm, shm_offset));
        });
    }

    fn read_io_ports(addr: u32, slice: &mut [T], emu: &mut Emu) {
        read_io_ports!(
            CPU,
            addr,
            addr_offset,
            {
                for i in 0..slice.len() {
                    slice[i] = match CPU {
                        ARM9 => emu.io_arm9_read(addr_offset),
                        ARM7 => emu.io_arm7_read(addr_offset),
                    };
                }
            },
            {
                slice.fill(emu.mem.wifi.read(addr_offset));
            }
        )
    }

    fn read_palettes(_: u32, _: &mut [T], _: &mut Emu) {
        unsafe { unreachable_unchecked() }
    }

    fn read_vram(addr: u32, slice: &mut [T], emu: &mut Emu) {
        slice.fill(emu.vram_read::<CPU, _>(addr));
    }

    fn write(addr: u32, slice: &[T], emu: &mut Emu) {
        write_dtcm!(CPU, TCM, addr, emu, shm_offset, {
            utils::write_to_mem(&mut emu.mem.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() })
        });
        let func = unsafe { Self::WRITE_LUT.get_unchecked(((addr >> 24) & 0xF) as usize) };
        func(addr, slice, emu);
    }

    fn write_itcm(addr: u32, slice: &[T], emu: &mut Emu) {
        write_itcm!(CPU, TCM, addr, size_of_val(slice), emu, shm_offset, {
            utils::write_to_mem(&mut emu.mem.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() });
        });
    }

    fn write_main(addr: u32, slice: &[T], emu: &mut Emu) {
        write_main!(addr, size_of_val(slice), emu, shm_offset, {
            utils::write_to_mem(&mut emu.mem.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() });
        });
    }

    fn write_wram(addr: u32, slice: &[T], emu: &mut Emu) {
        write_wram!(CPU, addr, size_of_val(slice), emu, shm_offset, {
            utils::write_to_mem(&mut emu.mem.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() });
        })
    }

    fn write_io_ports(addr: u32, slice: &[T], emu: &mut Emu) {
        write_io_ports!(
            CPU,
            addr,
            addr_offset,
            {
                match CPU {
                    ARM9 => emu.io_arm9_write_fixed_slice(addr_offset, slice),
                    ARM7 => emu.io_arm7_write_fixed_slice(addr_offset, slice),
                }
            },
            { emu.mem.wifi.write(addr_offset, unsafe { *slice.last().unwrap_unchecked() }) }
        );
    }

    fn write_palettes(_: u32, _: &[T], _: &mut Emu) {
        unsafe { unreachable_unchecked() }
    }

    fn write_vram(addr: u32, slice: &[T], emu: &mut Emu) {
        for i in 0..slice.len() {
            emu.vram_write::<CPU, _>(addr, slice[i]);
        }
    }
}

struct MemoryMultipleMemsetIo<const CPU: CpuType, const TCM: bool, T: Convert> {
    _data: PhantomData<T>,
}

impl<const CPU: CpuType, const TCM: bool, T: Convert> MemoryMultipleMemsetIo<CPU, TCM, T> {
    const WRITE_LUT: [fn(u32, T, usize, &mut Emu); 7] = create_io_write_lut!();

    fn write(addr: u32, value: T, size: usize, emu: &mut Emu) {
        write_dtcm!(CPU, TCM, addr, emu, shm_offset, {
            utils::write_memset(&mut emu.mem.shm, shm_offset as usize, value, size);
        });
        let func = unsafe { Self::WRITE_LUT.get_unchecked(((addr >> 24) & 0xF) as usize) };
        func(addr, value, size, emu);
    }

    fn write_itcm(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_itcm!(CPU, TCM, addr, size << write_shift, emu, shm_offset, {
            utils::write_memset(&mut emu.mem.shm, shm_offset as usize, value, size);
        });
    }

    fn write_main(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_main!(addr, size << write_shift, emu, shm_offset, {
            utils::write_memset(&mut emu.mem.shm, shm_offset as usize, value, size);
        });
    }

    fn write_wram(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_wram!(CPU, addr, size << write_shift, emu, shm_offset, {
            utils::write_memset(&mut emu.mem.shm, shm_offset as usize, value, size);
        });
    }

    fn write_io_ports(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        write_io_ports!(
            CPU,
            addr,
            addr_offset,
            {
                for i in 0..size {
                    match CPU {
                        ARM9 => emu.io_arm9_write(addr_offset + (i << write_shift) as u32, value),
                        ARM7 => emu.io_arm7_write(addr_offset + (i << write_shift) as u32, value),
                    }
                }
            },
            {
                emu.mem.wifi.write_memset(addr_offset, value, size);
            }
        )
    }

    fn write_palettes(_: u32, _: T, _: usize, _: &mut Emu) {
        unsafe { unreachable_unchecked() }
    }

    fn write_vram(addr: u32, value: T, size: usize, emu: &mut Emu) {
        let write_shift = size_of::<T>() >> 1;
        for i in 0..size {
            emu.vram_write::<CPU, _>(addr + (i << write_shift) as u32, value);
        }
    }
}

impl Memory {
    pub fn new() -> Self {
        Memory {
            shm: Shm::new("physical", regions::TOTAL_MEM_SIZE as usize).unwrap(),
            wram: Wram::new(),
            wifi: Wifi::new(),
            vram: Vram::default(),
            mmu_arm9: MmuArm9::new(),
            mmu_arm7: MmuArm7::new(),
        }
    }

    pub fn init(&mut self) {
        self.shm.fill(0);
        self.wram = Wram::new();
        self.wifi = Wifi::new();
        self.vram = Vram::default();
    }
}

impl Emu {
    pub fn mem_get_palettes(&self) -> &'static [u8; STANDARD_PALETTES_SIZE as usize] {
        unsafe { mem::transmute(self.mem.shm.as_ptr().add(regions::PALETTES_REGION.shm_offset)) }
    }

    pub fn mem_get_oam(&self) -> &'static [u8; OAM_SIZE as usize] {
        unsafe { mem::transmute(self.mem.shm.as_ptr().add(regions::OAM_REGION.shm_offset)) }
    }

    pub fn get_shm_offset<const CPU: CpuType, const TCM: bool, const WRITE: bool>(&self, addr: u32) -> usize {
        let mmu = {
            if CPU == ARM9 && TCM {
                if WRITE {
                    self.mmu_get_write_tcm::<CPU>()
                } else {
                    self.mmu_get_read_tcm::<CPU>()
                }
            } else if WRITE {
                self.mmu_get_write::<CPU>()
            } else {
                self.mmu_get_read::<CPU>()
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

    pub fn mem_read<const CPU: CpuType, T: Convert>(&mut self, addr: u32) -> T {
        self.mem_read_with_options::<CPU, true, T>(addr)
    }

    pub fn mem_read_no_tcm<const CPU: CpuType, T: Convert>(&mut self, addr: u32) -> T {
        self.mem_read_with_options::<CPU, false, T>(addr)
    }

    pub fn mem_read_with_options<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32) -> T {
        debug_println!("{CPU:?} memory read at {addr:x}");
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, false>(aligned_addr) as u32;
        if shm_offset != 0 {
            let ret: T = utils::read_from_mem(&self.mem.shm, shm_offset);
            debug_println!("{CPU:?} memory read at {addr:x} with value {:x}", ret.into());
            return ret;
        }

        let ret: T = MemoryIo::<CPU, TCM, T>::read(aligned_addr, self);
        debug_println!("{CPU:?} memory read at {addr:x} with value {:x}", ret.into());
        ret
    }

    pub fn mem_read_multiple_slice<const CPU: CpuType, const TCM: bool, const SHM_MEMORY: bool, T: Convert>(&mut self, addr: u32, slice: &mut [T]) {
        debug_println!("{CPU:?} slice memory read at {addr:x} with size {}", size_of_val(slice));
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        if SHM_MEMORY {
            let shm_offset = self.get_shm_offset::<CPU, TCM, false>(aligned_addr) as u32;
            if shm_offset != 0 {
                utils::read_from_mem_slice(&self.mem.shm, shm_offset, slice);
                if DEBUG_LOG {
                    for (i, &value) in slice.iter().enumerate() {
                        debug_println!("{CPU:?} slice memory read at {:x} with value {:x}", aligned_addr as usize + i * size_of::<T>(), value.into());
                    }
                }
                return;
            }
        }

        MemoryMultipleSliceIo::<CPU, TCM, T>::read(aligned_addr, slice, self);

        if DEBUG_LOG {
            for (i, &value) in slice.iter().enumerate() {
                debug_println!("{CPU:?} slice memory read at {:x} with value {:x}", aligned_addr as usize + i * size_of::<T>(), value.into());
            }
        }
    }

    pub fn mem_read_fixed_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, slice: &mut [T]) {
        debug_println!("{CPU:?} fixed slice memory read at {addr:x} with size {}", size_of_val(slice));
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, false>(aligned_addr) as u32;
        if shm_offset != 0 {
            slice.fill(utils::read_from_mem(&self.mem.shm, shm_offset));
        } else {
            MemoryFixedSliceIo::<CPU, TCM, T>::read(aligned_addr, slice, self);
        }

        if DEBUG_LOG {
            for &mut value in slice {
                debug_println!("{CPU:?} fixed slice memory read at {:x} with value {:x}", aligned_addr as usize, value.into());
            }
        }
    }

    pub fn mem_write<const CPU: CpuType, T: Convert>(&mut self, addr: u32, value: T) {
        self.mem_write_internal::<CPU, true, T>(addr, value)
    }

    pub fn mem_write_no_tcm<const CPU: CpuType, T: Convert>(&mut self, addr: u32, value: T) {
        self.mem_write_internal::<CPU, false, T>(addr, value)
    }

    fn mem_write_internal<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, value: T) {
        debug_println!("{:?} memory write at {:x} with value {:x}", CPU, addr, value.into());
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr);
        if shm_offset != 0 {
            utils::write_to_mem(&mut self.mem.shm, shm_offset as u32, value);
            return;
        }

        MemoryIo::<CPU, TCM, T>::write(aligned_addr, value, self);
    }

    pub fn mem_write_multiple_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, slice: &[T]) {
        debug_println!("{CPU:?} fixed slice memory write at {addr:x} with size {}", size_of_val(slice));
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;
        if DEBUG_LOG {
            for (i, &value) in slice.iter().enumerate() {
                debug_println!("{CPU:?} slice memory write at {:x} with value {:x}", aligned_addr as usize + i * size_of::<T>(), value.into());
            }
        }

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            utils::write_to_mem_slice(&mut self.mem.shm, shm_offset as usize, slice);
            return;
        }

        MemoryMultipleSliceIo::<CPU, TCM, T>::write(aligned_addr, slice, self);
    }

    pub fn mem_write_fixed_slice<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, slice: &[T]) {
        debug_println!("{CPU:?} fixed slice memory write at {addr:x} with size {}", size_of_val(slice));
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;
        if DEBUG_LOG {
            for &value in slice {
                debug_println!("{CPU:?} fixed slice memory write at {:x} with value {:x}", aligned_addr, value.into());
            }
        }

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            utils::write_to_mem(&mut self.mem.shm, shm_offset, unsafe { slice.last().unwrap_unchecked() });
            return;
        }

        MemoryFixedSliceIo::<CPU, TCM, T>::write(aligned_addr, slice, self);
    }

    pub fn mem_write_multiple_memset<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32, value: T, size: usize) {
        debug_println!("{CPU:?} multiple memset memory write at {addr:x} with size {}", size_of::<T>() * size);
        let aligned_addr = addr & !(size_of::<T>() as u32 - 1);
        let aligned_addr = aligned_addr & 0x0FFFFFFF;

        let shm_offset = self.get_shm_offset::<CPU, TCM, true>(aligned_addr) as u32;
        if shm_offset != 0 {
            utils::write_memset(&mut self.mem.shm, shm_offset as usize, value, size);
            return;
        }

        MemoryMultipleMemsetIo::<CPU, TCM, T>::write(aligned_addr, value, size, self);
    }

    pub fn mem_read_struct<const CPU: CpuType, const TCM: bool, T>(&mut self, addr: u32) -> T
    where
        [(); size_of::<T>()]:,
    {
        let mut mem = [0; size_of::<T>()];
        self.mem_read_multiple_slice::<CPU, TCM, true, u8>(addr, &mut mem);
        unsafe { mem::transmute_copy(&mem) }
    }

    pub fn mem_write_struct<const CPU: CpuType, const TCM: bool, T>(&mut self, addr: u32, value: &T) {
        let slice = unsafe { core::slice::from_raw_parts(value as *const T as *const u8, size_of::<T>()) };
        self.mem_write_multiple_slice::<CPU, TCM, u8>(addr, slice);
    }
}

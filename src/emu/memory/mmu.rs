use crate::emu::cp15::TcmState;
use crate::emu::emu::{get_cp15, get_mem, Emu};
use crate::emu::memory::{regions, vram};
use crate::emu::CpuType::{ARM7, ARM9};
use crate::utils::HeapMemU32;
use std::cell::UnsafeCell;
use std::cmp::max;
use std::ops::DerefMut;

const MMU_BLOCK_SHIFT: u32 = 12;
pub const MMU_BLOCK_SIZE: u32 = 1 << MMU_BLOCK_SHIFT;
pub const MMU_SIZE: usize = ((1u64 << 32) / MMU_BLOCK_SIZE as u64) as usize;

pub trait Mmu {
    fn update_all(&self, emu: &Emu);
    fn update_itcm(&self, emu: &Emu);
    fn update_dtcm(&self, emu: &Emu);
    fn update_wram(&self, emu: &Emu);
    fn update_vram(&self, emu: &Emu);
    fn get_base_ptr(&self, addr: u32) -> *const u8;
    fn get_mmu_ptr(&self) -> *const u32;
}

struct MmuArm9Inner {
    read_map: HeapMemU32<MMU_SIZE>,
    current_itcm_size: u32,
    current_dtcm_addr: u32,
    current_dtcm_size: u32,
}

impl MmuArm9Inner {
    fn new() -> Self {
        MmuArm9Inner {
            read_map: HeapMemU32::new(),
            current_itcm_size: 0,
            current_dtcm_addr: 0,
            current_dtcm_size: 0,
        }
    }

    fn update(&mut self, start: u32, end: u32, emu: &Emu) {
        for addr in (start..end).step_by(MMU_BLOCK_SIZE as usize) {
            let read_ptr = &mut self.read_map[(addr >> MMU_BLOCK_SHIFT) as usize];
            *read_ptr = 0;

            match addr & 0xFF000000 {
                regions::MAIN_MEMORY_OFFSET => *read_ptr = get_mem!(emu).main.get_ptr(addr) as u32,
                regions::SHARED_WRAM_OFFSET => *read_ptr = get_mem!(emu).wram.get_ptr::<{ ARM9 }>(addr) as u32,
                // regions::VRAM_OFFSET => *read_ptr = emu.mem.vram.get_ptr::<{ ARM9 }>(addr) as u32,
                0xFF000000 => {
                    if addr & 0xFFFF8000 == regions::ARM9_BIOS_OFFSET {
                        *read_ptr = get_mem!(emu).bios_arm9.get_ptr(addr) as u32
                    }
                }
                _ => {}
            }

            let cp15 = get_cp15!(emu, ARM9);
            if addr < cp15.itcm_size {
                if cp15.itcm_state == TcmState::RW {
                    *read_ptr = get_mem!(emu).tcm.get_itcm_ptr(addr) as u32;
                }
            } else if addr >= cp15.dtcm_addr && addr < cp15.dtcm_addr + cp15.dtcm_size && cp15.dtcm_state == TcmState::RW {
                *read_ptr = get_mem!(emu).tcm.get_dtcm_ptr(addr) as u32;
            }
        }

        let cp15 = get_cp15!(emu, ARM9);
        self.current_itcm_size = cp15.itcm_size;
        self.current_dtcm_addr = cp15.dtcm_addr;
        self.current_dtcm_size = cp15.dtcm_size;
    }

    fn get_base_ptr(&self, addr: u32) -> *const u8 {
        self.read_map[(addr >> MMU_BLOCK_SHIFT) as usize] as _
    }
}

pub struct MmuArm9 {
    inner: UnsafeCell<MmuArm9Inner>,
}

impl MmuArm9 {
    pub fn new() -> Self {
        MmuArm9 {
            inner: UnsafeCell::new(MmuArm9Inner::new()),
        }
    }
}

impl Mmu for MmuArm9 {
    #[inline(never)]
    fn update_all(&self, emu: &Emu) {
        unsafe { (*self.inner.get()).update(0, u32::MAX, emu) };
    }

    fn update_itcm(&self, emu: &Emu) {
        let inner = unsafe { self.inner.get().as_mut().unwrap_unchecked() };
        inner.update(regions::INSTRUCTION_TCM_OFFSET, max(inner.current_itcm_size, get_cp15!(emu, ARM9).itcm_size), emu);
    }

    fn update_dtcm(&self, emu: &Emu) {
        let inner = unsafe { self.inner.get().as_mut().unwrap_unchecked() };
        inner.update(inner.current_dtcm_addr, inner.current_dtcm_addr + inner.current_dtcm_size, emu);
        let cp15 = get_cp15!(emu, ARM9);
        inner.update(cp15.dtcm_addr, cp15.dtcm_addr + cp15.dtcm_size, emu);
    }

    fn update_wram(&self, emu: &Emu) {
        unsafe { (*self.inner.get()).update(regions::SHARED_WRAM_OFFSET, regions::IO_PORTS_OFFSET, emu) };
    }

    fn update_vram(&self, emu: &Emu) {
        let read_map = unsafe { (*self.inner.get()).read_map.deref_mut() };

        macro_rules! update_range {
            ($start:expr, $end:expr) => {{
                for addr in ($start..$end).step_by(MMU_BLOCK_SIZE as usize) {
                    let read_ptr = &mut read_map[(addr >> MMU_BLOCK_SHIFT) as usize];
                    *read_ptr = get_mem!(emu).vram.get_ptr::<{ ARM9 }>(addr) as u32;
                }
            }};
        }

        update_range!(regions::VRAM_OFFSET + vram::BG_A_OFFSET, regions::VRAM_OFFSET + vram::BG_A_OFFSET + vram::BG_A_SIZE);
        update_range!(regions::VRAM_OFFSET + vram::BG_B_OFFSET, regions::VRAM_OFFSET + vram::BG_B_OFFSET + vram::BG_B_SIZE);
        update_range!(regions::VRAM_OFFSET + vram::OBJ_A_OFFSET, regions::VRAM_OFFSET + vram::OBJ_A_OFFSET + vram::OBJ_A_SIZE);
        update_range!(regions::VRAM_OFFSET + vram::OBJ_B_OFFSET, regions::VRAM_OFFSET + vram::OBJ_B_OFFSET + vram::OBJ_B_OFFSET);
        update_range!(regions::VRAM_OFFSET + vram::LCDC_OFFSET, regions::VRAM_OFFSET + vram::LCDC_OFFSET + vram::TOTAL_SIZE as u32);
    }

    fn get_base_ptr(&self, addr: u32) -> *const u8 {
        unsafe { (*self.inner.get()).get_base_ptr(addr) }
    }

    fn get_mmu_ptr(&self) -> *const u32 {
        unsafe { (*self.inner.get()).read_map.as_ptr() }
    }
}

struct MmuArm7Inner {
    read_map: HeapMemU32<MMU_SIZE>,
}

impl MmuArm7Inner {
    fn new() -> Self {
        MmuArm7Inner { read_map: HeapMemU32::new() }
    }

    fn update(&mut self, start: u32, end: u32, emu: &Emu) {
        for addr in (start..end).step_by(MMU_BLOCK_SIZE as usize) {
            let read_ptr = &mut self.read_map[(addr >> MMU_BLOCK_SHIFT) as usize];
            *read_ptr = 0;

            match addr & 0xFF000000 {
                regions::ARM7_BIOS_OFFSET => {
                    if addr < regions::ARM7_BIOS_SIZE {
                        *read_ptr = get_mem!(emu).bios_arm7.get_ptr(addr) as u32
                    }
                }
                regions::MAIN_MEMORY_OFFSET => *read_ptr = get_mem!(emu).main.get_ptr(addr) as u32,
                regions::SHARED_WRAM_OFFSET => *read_ptr = get_mem!(emu).wram.get_ptr::<{ ARM7 }>(addr) as u32,
                // regions::VRAM_OFFSET => *read_ptr = emu.mem.vram.get_ptr::<{ ARM7 }>(addr) as u32,
                _ => {}
            }
        }
    }

    fn get_base_ptr(&self, addr: u32) -> *const u8 {
        self.read_map[(addr >> MMU_BLOCK_SHIFT) as usize] as _
    }
}

pub struct MmuArm7 {
    inner: UnsafeCell<MmuArm7Inner>,
}

impl MmuArm7 {
    pub fn new() -> Self {
        MmuArm7 {
            inner: UnsafeCell::new(MmuArm7Inner::new()),
        }
    }
}

impl Mmu for MmuArm7 {
    #[inline(never)]
    fn update_all(&self, emu: &Emu) {
        unsafe { (*self.inner.get()).update(0, u32::MAX, emu) };
    }

    fn update_itcm(&self, _: &Emu) {
        unreachable!()
    }

    fn update_dtcm(&self, _: &Emu) {
        unreachable!()
    }

    fn update_wram(&self, emu: &Emu) {
        unsafe { (*self.inner.get()).update(regions::SHARED_WRAM_OFFSET, regions::IO_PORTS_OFFSET, emu) };
    }

    fn update_vram(&self, emu: &Emu) {
        let read_map = unsafe { (*self.inner.get()).read_map.deref_mut() };
        for addr in (regions::VRAM_OFFSET..0x6200000).step_by(MMU_BLOCK_SIZE as usize) {
            let read_ptr = &mut read_map[(addr >> MMU_BLOCK_SHIFT) as usize];
            *read_ptr = get_mem!(emu).vram.get_ptr::<{ ARM7 }>(addr) as u32;
        }
    }

    fn get_base_ptr(&self, addr: u32) -> *const u8 {
        unsafe { (*self.inner.get()).get_base_ptr(addr) }
    }

    fn get_mmu_ptr(&self) -> *const u32 {
        unsafe { (*self.inner.get()).read_map.as_ptr() }
    }
}

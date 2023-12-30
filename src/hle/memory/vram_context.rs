use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils;
use crate::utils::HeapMemU8;
use bilge::prelude::*;
use static_assertions::const_assert_eq;
use std::cmp::min;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::{mem, ptr, slice};

const BANK_SIZE: usize = 9;

#[derive(Copy, Clone)]
struct VramMap<const SIZE: usize> {
    ptr: *const u8,
}

impl<const SIZE: usize> VramMap<SIZE> {
    fn new<const T: usize>(heap: &HeapMemU8<T>) -> Self {
        VramMap {
            ptr: heap.as_ptr() as _,
        }
    }

    fn extract_section<const CHUNK_SIZE: usize>(&self, offset: usize) -> VramMap<CHUNK_SIZE> {
        VramMap::from((self.ptr as usize + CHUNK_SIZE * offset) as *const u8)
    }

    fn as_mut(&mut self) -> VramMapMut<SIZE> {
        VramMapMut::new(self.ptr as _)
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    pub const fn len(&self) -> usize {
        SIZE
    }
}

impl<const SIZE: usize> Deref for VramMap<SIZE> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl<const SIZE: usize> AsRef<[u8]> for VramMap<SIZE> {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl<'a, const SIZE: usize> Default for VramMap<SIZE> {
    fn default() -> Self {
        VramMap {
            ptr: ptr::null_mut(),
        }
    }
}

impl<'a, const SIZE: usize> From<*const u8> for VramMap<SIZE> {
    fn from(value: *const u8) -> Self {
        VramMap { ptr: value }
    }
}

struct VramMapMut<const SIZE: usize> {
    ptr: *mut u8,
}

impl<const SIZE: usize> VramMapMut<SIZE> {
    fn new(ptr: *mut u8) -> Self {
        VramMapMut { ptr }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    pub const fn len(&self) -> usize {
        SIZE
    }
}

impl<const SIZE: usize> Deref for VramMapMut<SIZE> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl<const SIZE: usize> AsRef<[u8]> for VramMapMut<SIZE> {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl<const SIZE: usize> DerefMut for VramMapMut<SIZE> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }
}

impl<const SIZE: usize> AsMut<[u8]> for VramMapMut<SIZE> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

#[derive(Copy, Clone, Default)]
struct OverlapSection<const SIZE: usize> {
    overlaps: [VramMap<SIZE>; 4],
    count: usize,
}

impl<const SIZE: usize> OverlapSection<SIZE> {
    fn add(&mut self, map: VramMap<SIZE>) {
        self.overlaps[self.count] = map;
        self.count += 1;
    }

    pub fn read_slice<T: utils::Convert>(&self, index: u32, slice: &mut [T]) {
        let mut buf = vec![T::from(0); slice.len()];
        for i in 0..self.count {
            let map = &self.overlaps[i];
            debug_assert_ne!(map.ptr, ptr::null());
            let read_amount = utils::read_from_mem_slice(&map, index, &mut buf);
            slice[..read_amount]
                .iter_mut()
                .zip(&buf[..read_amount])
                .for_each(|(a, b)| *a = T::from((*a).into() | (*b).into()))
        }
    }

    pub fn write_slice<T: utils::Convert>(&mut self, index: u32, slice: &[T]) {
        for i in 0..self.count {
            let map = &mut self.overlaps[i];
            let mut map = map.as_mut();
            debug_assert_ne!(map.ptr, ptr::null_mut());
            utils::write_to_mem_slice(&mut map, index, slice);
        }
    }
}

#[derive(Copy, Clone)]
struct OverlapMapping<const SIZE: usize, const CHUNK_SIZE: usize, const SECTIONS_COUNT: usize> {
    sections: [OverlapSection<CHUNK_SIZE>; SECTIONS_COUNT],
}

impl<const SIZE: usize, const CHUNK_SIZE: usize, const SECTIONS_COUNT: usize>
    OverlapMapping<SIZE, CHUNK_SIZE, SECTIONS_COUNT>
{
    fn new() -> Self {
        OverlapMapping {
            sections: [OverlapSection::default(); SECTIONS_COUNT],
        }
    }

    fn add<const MAP_SIZE: usize>(&mut self, map: VramMap<MAP_SIZE>, offset: usize) {
        for i in 0..(MAP_SIZE / CHUNK_SIZE) {
            self.sections[offset + i].add(map.extract_section::<CHUNK_SIZE>(i))
        }
    }

    pub fn read_slice<T: utils::Convert>(&self, mut addr: u32, slice: &mut [T]) -> usize {
        addr &= SIZE as u32 - 1;
        debug_assert!(addr as usize + slice.len() < SIZE);

        let mut slice_index = 0;
        while slice_index < slice.len() {
            let section_index = (addr as usize + slice_index * mem::size_of::<T>()) / CHUNK_SIZE;
            let section_offset = (addr as usize + slice_index * mem::size_of::<T>()) % CHUNK_SIZE;

            let read_amount = min(
                (CHUNK_SIZE - section_offset) / mem::size_of::<T>(),
                slice.len() - slice_index,
            );
            self.sections[section_index].read_slice(
                section_offset as u32,
                &mut slice[slice_index..slice_index + read_amount],
            );
            slice_index += read_amount;
        }
        slice_index
    }

    pub fn write_slice<T: utils::Convert>(&mut self, mut addr: u32, slice: &[T]) -> usize {
        addr &= SIZE as u32 - 1;
        debug_assert!(addr as usize + slice.len() < SIZE);

        let mut slice_index = 0;
        while slice_index < slice.len() {
            let section_index = (addr as usize + slice_index * mem::size_of::<T>()) / CHUNK_SIZE;
            let section_offset = (addr as usize + slice_index * mem::size_of::<T>()) % CHUNK_SIZE;

            let write_amount = min(
                (CHUNK_SIZE - section_offset) / mem::size_of::<T>(),
                slice.len() - slice_index,
            );
            self.sections[section_index].write_slice(
                section_offset as u32,
                &slice[slice_index..slice_index + write_amount],
            );
            slice_index += write_amount;
        }
        slice_index
    }
}

#[bitsize(8)]
#[derive(FromBits)]
struct VramCnt {
    mst: u3,
    ofs: u2,
    not_used: u2,
    enable: u1,
}

const BANK_A_SIZE: usize = 128 * 1024;
const BANK_B_SIZE: usize = BANK_A_SIZE;
const BANK_C_SIZE: usize = BANK_A_SIZE;
const BANK_D_SIZE: usize = BANK_A_SIZE;
const BANK_E_SIZE: usize = 64 * 1024;
const BANK_F_SIZE: usize = 16 * 1024;
const BANK_G_SIZE: usize = 16 * 1024;
const BANK_H_SIZE: usize = 32 * 1024;
const BANK_I_SIZE: usize = 16 * 1024;
const TOTAL_SIZE: usize = BANK_A_SIZE
    + BANK_B_SIZE
    + BANK_C_SIZE
    + BANK_D_SIZE
    + BANK_E_SIZE
    + BANK_F_SIZE
    + BANK_G_SIZE
    + BANK_H_SIZE
    + BANK_I_SIZE;
const_assert_eq!(TOTAL_SIZE, 656 * 1024);

struct VramBanks {
    vram_a: HeapMemU8<BANK_A_SIZE>,
    vram_b: HeapMemU8<BANK_B_SIZE>,
    vram_c: HeapMemU8<BANK_C_SIZE>,
    vram_d: HeapMemU8<BANK_D_SIZE>,
    vram_e: HeapMemU8<BANK_E_SIZE>,
    vram_f: HeapMemU8<BANK_F_SIZE>,
    vram_g: HeapMemU8<BANK_G_SIZE>,
    vram_h: HeapMemU8<BANK_H_SIZE>,
    vram_i: HeapMemU8<BANK_I_SIZE>,
}

impl VramBanks {
    fn new() -> Self {
        let instance = VramBanks {
            vram_a: HeapMemU8::new(),
            vram_b: HeapMemU8::new(),
            vram_c: HeapMemU8::new(),
            vram_d: HeapMemU8::new(),
            vram_e: HeapMemU8::new(),
            vram_f: HeapMemU8::new(),
            vram_g: HeapMemU8::new(),
            vram_h: HeapMemU8::new(),
            vram_i: HeapMemU8::new(),
        };

        debug_println!(
            "Allocating vram banks at a: {:x}, b: {:x}, c: {:x}, d: {:x}, e: {:x}, f: {:x}, g: {:x}, h: {:x}, i: {:x}",
            instance.vram_a.as_ptr() as u32, instance.vram_b.as_ptr() as u32,
            instance.vram_c.as_ptr() as u32, instance.vram_d.as_ptr() as u32,
            instance.vram_e.as_ptr() as u32, instance.vram_f.as_ptr() as u32,
            instance.vram_g.as_ptr() as u32, instance.vram_h.as_ptr() as u32,
            instance.vram_i.as_ptr() as u32
        );

        instance
    }
}

const LCDC_OFFSET: u32 = 0x800000;
const BG_A_OFFSET: u32 = 0x000000;
const OBJ_A_OFFSET: u32 = 0x400000;
const BG_B_OFFSET: u32 = 0x200000;
const OBJ_B_OFFSET: u32 = 0x600000;

struct VramInner {
    stat: Arc<AtomicU8>,
    cnt: [u8; BANK_SIZE],
    banks: VramBanks,

    lcdc: OverlapMapping<TOTAL_SIZE, { 16 * 1024 }, { TOTAL_SIZE / 1024 / 16 }>,

    bg_a: OverlapMapping<{ 512 * 1024 }, { 16 * 1024 }, { 512 / 16 }>,
    obj_a: OverlapMapping<{ 256 * 1024 }, { 16 * 1024 }, { 256 / 16 }>,
    bg_ext_palette_a: [VramMap<{ 16 * 1024 }>; 64 / 16],
    obj_ext_palette_a: VramMap<{ 16 * 1024 }>,

    tex_rear_plane_img: [VramMap<{ 128 * 1024 }>; 4],
    tex_palette: [VramMap<{ 16 * 1024 }>; 6],

    bg_b: OverlapMapping<{ 128 * 1024 }, { 16 * 1024 }, { 128 / 16 }>,
    obj_b: OverlapMapping<{ 128 * 1024 }, { 16 * 1024 }, { 128 / 16 }>,
    bg_ext_palette_b: VramMap<{ 32 * 1024 }>,
    obj_ext_palette_b: VramMap<{ 16 * 1024 }>,

    arm7: OverlapMapping<{ 128 * 2 * 1024 }, { 128 * 1024 }, 2>,
}

impl VramInner {
    fn new(stat: Arc<AtomicU8>) -> Self {
        let instance = VramInner {
            stat,
            cnt: [0u8; BANK_SIZE],
            banks: VramBanks::new(),

            lcdc: OverlapMapping::new(),

            bg_a: OverlapMapping::new(),
            obj_a: OverlapMapping::new(),
            bg_ext_palette_a: [VramMap::default(); 64 / 16],
            obj_ext_palette_a: VramMap::default(),

            tex_rear_plane_img: [VramMap::default(); 4],
            tex_palette: [VramMap::default(); 6],

            bg_b: OverlapMapping::new(),
            obj_b: OverlapMapping::new(),
            bg_ext_palette_b: VramMap::default(),
            obj_ext_palette_b: VramMap::default(),

            arm7: OverlapMapping::new(),
        };
        debug_assert_eq!(
            instance.banks.vram_a.len()
                + instance.banks.vram_b.len()
                + instance.banks.vram_c.len()
                + instance.banks.vram_d.len()
                + instance.banks.vram_e.len()
                + instance.banks.vram_f.len()
                + instance.banks.vram_g.len()
                + instance.banks.vram_h.len()
                + instance.banks.vram_i.len(),
            TOTAL_SIZE
        );
        instance
    }

    pub fn set_cnt(&mut self, bank: usize, value: u8) {
        debug_println!("Set vram cnt {:x} to {:x}", bank, value);
        const MASKS: [u8; 9] = [0x9B, 0x9B, 0x9F, 0x9F, 0x87, 0x9F, 0x9F, 0x83, 0x83];
        let value = value & MASKS[bank];
        if self.cnt[bank] == value {
            return;
        }
        self.cnt[bank] = value;

        self.lcdc = OverlapMapping::new();
        self.bg_a = OverlapMapping::new();
        self.obj_a = OverlapMapping::new();
        self.bg_ext_palette_a.fill(VramMap::default());
        self.obj_ext_palette_a = VramMap::default();
        self.tex_rear_plane_img.fill(VramMap::default());
        self.tex_palette.fill(VramMap::default());
        self.bg_b = OverlapMapping::new();
        self.obj_b = OverlapMapping::new();
        self.bg_ext_palette_b = VramMap::default();
        self.obj_ext_palette_b = VramMap::default();
        self.arm7 = OverlapMapping::new();

        {
            let cnt_a = VramCnt::from(self.cnt[0]);
            if bool::from(cnt_a.enable()) {
                let mst = u8::from(cnt_a.mst()) & 0x3;
                match mst {
                    0 => {
                        let map: VramMap<BANK_A_SIZE> = VramMap::new(&self.banks.vram_a);
                        self.lcdc.add::<BANK_A_SIZE>(map, 0);
                    }
                    1 => {
                        let ofs = u8::from(cnt_a.ofs());
                        self.bg_a.add::<BANK_A_SIZE>(
                            VramMap::new(&self.banks.vram_a),
                            128 / 16 * ofs as usize,
                        );
                    }
                    2 => {
                        todo!()
                    }
                    3 => {
                        todo!()
                    }
                    _ => {}
                }
            }
        }

        {
            let cnt_b = VramCnt::from(self.cnt[1]);
            if bool::from(cnt_b.enable()) {
                let mst = u8::from(cnt_b.mst()) & 0x3;
                match mst {
                    0 => {
                        self.lcdc.add::<BANK_B_SIZE>(
                            VramMap::new(&self.banks.vram_b),
                            BANK_A_SIZE / 1024 / 16,
                        );
                    }
                    1 => {
                        let ofs = u8::from(cnt_b.ofs());
                        self.bg_a.add::<BANK_B_SIZE>(
                            VramMap::new(&self.banks.vram_b),
                            128 / 16 * ofs as usize,
                        );
                    }
                    2 => {
                        let ofs = u8::from(cnt_b.ofs());
                        self.obj_a.add::<BANK_B_SIZE>(
                            VramMap::new(&self.banks.vram_b),
                            128 / 16 * (ofs & 1) as usize,
                        );
                    }
                    3 => {
                        todo!()
                    }
                    _ => {}
                }
            }
        }

        {
            let cnt_c = VramCnt::from(self.cnt[2]);
            if bool::from(cnt_c.enable()) {
                let mst = u8::from(cnt_c.mst());
                match mst {
                    0 => {
                        self.lcdc.add::<BANK_C_SIZE>(
                            VramMap::new(&self.banks.vram_c),
                            BANK_A_SIZE / 1024 / 16 * 2,
                        );
                    }
                    1 => {
                        let ofs = u8::from(cnt_c.ofs());
                        self.bg_a.add::<BANK_C_SIZE>(
                            VramMap::new(&self.banks.vram_c),
                            128 / 16 * ofs as usize,
                        );
                    }
                    2 => {
                        todo!()
                    }
                    3 => {
                        todo!()
                    }
                    4 => {
                        self.bg_b
                            .add::<BANK_C_SIZE>(VramMap::new(&self.banks.vram_c), 0);
                    }
                    _ => {}
                }
            }
        }

        {
            let cnt_d = VramCnt::from(self.cnt[3]);
            if bool::from(cnt_d.enable()) {
                let mst = u8::from(cnt_d.mst());
                match mst {
                    0 => {
                        self.lcdc.add::<BANK_D_SIZE>(
                            VramMap::new(&self.banks.vram_d),
                            BANK_A_SIZE / 1024 / 16 * 3,
                        );
                    }
                    1 => {
                        let ofs = u8::from(cnt_d.ofs());
                        self.bg_a.add::<BANK_D_SIZE>(
                            VramMap::new(&self.banks.vram_d),
                            128 / 16 * ofs as usize,
                        );
                    }
                    2 => {
                        todo!()
                    }
                    3 => {
                        todo!()
                    }
                    _ => {}
                }
            }
        }

        {
            let cnt_e = VramCnt::from(self.cnt[4]);
            if bool::from(cnt_e.enable()) {
                let mst = u8::from(cnt_e.mst());
                match mst {
                    0 => {
                        self.lcdc.add::<BANK_E_SIZE>(
                            VramMap::new(&self.banks.vram_e),
                            BANK_A_SIZE / 1024 / 16 * 4,
                        );
                    }
                    1 => {
                        self.bg_a
                            .add::<BANK_E_SIZE>(VramMap::new(&self.banks.vram_e), 0);
                    }
                    2 => {
                        todo!()
                    }
                    3 => {
                        todo!()
                    }
                    _ => {}
                }
            }
        }

        let cnt_f = VramCnt::from(self.cnt[5]);
        if bool::from(cnt_f.enable()) {
            let mst = u8::from(cnt_f.mst());
            match mst {
                0 => {
                    self.lcdc.add::<BANK_F_SIZE>(
                        VramMap::new(&self.banks.vram_f),
                        (BANK_A_SIZE * 4 + BANK_E_SIZE) / 1024 / 16,
                    );
                }
                1 => {
                    let ofs = u8::from(cnt_f.ofs()) as usize;
                    self.bg_a.add::<BANK_F_SIZE>(
                        VramMap::new(&self.banks.vram_f),
                        (ofs & 1) + (64 / 16 * (ofs & 0x2)),
                    );
                }
                2 => {
                    todo!()
                }
                3 => {
                    todo!()
                }
                4 => {
                    todo!()
                }
                5 => {
                    todo!()
                }
                _ => {}
            }
        }

        let cnt_g = VramCnt::from(self.cnt[6]);
        if bool::from(cnt_g.enable()) {
            let mst = u8::from(cnt_g.mst());
            match mst {
                0 => {
                    self.lcdc.add::<BANK_G_SIZE>(
                        VramMap::new(&self.banks.vram_g),
                        (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE) / 1024 / 16,
                    );
                }
                1 => {
                    let ofs = u8::from(cnt_g.ofs()) as usize;
                    self.bg_a.add::<BANK_G_SIZE>(
                        VramMap::new(&self.banks.vram_g),
                        (ofs & 1) + (64 / 16 * (ofs & 0x2)),
                    );
                }
                2 => {
                    todo!()
                }
                3 => {
                    todo!()
                }
                4 => {
                    todo!()
                }
                5 => {
                    todo!()
                }
                _ => {}
            }
        }

        let cnt_h = VramCnt::from(self.cnt[7]);
        if bool::from(cnt_h.enable()) {
            let mst = u8::from(cnt_h.mst()) & 0x3;
            match mst {
                0 => {
                    self.lcdc.add::<BANK_H_SIZE>(
                        VramMap::new(&self.banks.vram_h),
                        (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE) / 1024 / 16,
                    );
                }
                1 => {
                    todo!()
                }
                2 => {
                    todo!()
                }
                _ => {}
            }
        }

        let cnt_i = VramCnt::from(self.cnt[8]);
        if bool::from(cnt_i.enable()) {
            let mst = u8::from(cnt_i.mst()) & 0x3;
            match mst {
                0 => {
                    self.lcdc.add::<BANK_I_SIZE>(
                        VramMap::new(&self.banks.vram_i),
                        (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE + BANK_H_SIZE)
                            / 1024
                            / 16,
                    );
                }
                1 => {
                    todo!()
                }
                2 => {
                    todo!()
                }
                3 => {
                    todo!()
                }
                _ => {}
            }
        }
    }

    pub fn read_slice<const CPU: CpuType, T: utils::Convert>(
        &self,
        addr: u32,
        slice: &mut [T],
    ) -> usize {
        match CPU {
            CpuType::ARM9 => match addr & 0xE00000 {
                LCDC_OFFSET => self.lcdc.read_slice(addr, slice),
                BG_A_OFFSET => {
                    todo!()
                }
                OBJ_A_OFFSET => {
                    todo!()
                }
                BG_B_OFFSET => {
                    todo!()
                }
                OBJ_B_OFFSET => {
                    todo!()
                }
                _ => {
                    todo!("{:x}", addr)
                }
            },
            CpuType::ARM7 => {
                todo!()
            }
        }
    }

    pub fn write_slice<const CPU: CpuType, T: utils::Convert>(
        &mut self,
        addr: u32,
        slice: &[T],
    ) -> usize {
        match CPU {
            CpuType::ARM9 => match addr & 0xE00000 {
                LCDC_OFFSET => self.lcdc.write_slice(addr, slice),
                BG_A_OFFSET => {
                    todo!()
                }
                OBJ_A_OFFSET => {
                    todo!()
                }
                BG_B_OFFSET => {
                    todo!()
                }
                OBJ_B_OFFSET => {
                    todo!()
                }
                _ => {
                    todo!("{:x}", addr)
                }
            },
            CpuType::ARM7 => {
                todo!()
            }
        }
    }
}

pub struct VramContext {
    stat: Arc<AtomicU8>,
    inner: RwLock<VramInner>,
}

impl VramContext {
    pub fn new() -> Self {
        let stat = Arc::new(AtomicU8::new(0));
        VramContext {
            stat: stat.clone(),
            inner: RwLock::new(VramInner::new(stat)),
        }
    }

    pub fn get_stat(&self) -> u8 {
        self.stat.load(Ordering::Relaxed)
    }

    pub fn get_cnt(&self, bank: usize) -> u8 {
        self.inner.read().unwrap().cnt[bank]
    }

    pub fn set_cnt(&self, bank: usize, value: u8) {
        self.inner.write().unwrap().set_cnt(bank, value);
    }

    pub fn read_slice<const CPU: CpuType, T: utils::Convert>(
        &self,
        addr_offset: u32,
        slice: &mut [T],
    ) -> usize {
        self.inner
            .read()
            .unwrap()
            .read_slice::<CPU, _>(addr_offset, slice)
    }

    pub fn write_slice<const CPU: CpuType, T: utils::Convert>(
        &self,
        addr_offset: u32,
        slice: &[T],
    ) -> usize {
        self.inner
            .write()
            .unwrap()
            .write_slice::<CPU, _>(addr_offset, slice)
    }
}

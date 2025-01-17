use crate::core::emu::{get_jit_mut, Emu};
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::logging::debug_println;
use crate::utils;
use crate::utils::HeapMemU8;
use bilge::prelude::*;
use paste::paste;
use static_assertions::{const_assert, const_assert_eq};
use std::cmp::min;
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::ops::{Deref, DerefMut};
use std::{ptr, slice};

const BANK_SIZE: usize = 9;

static mut VRAM_BANK_START: usize = 0;

#[derive(Copy, Clone)]
struct VramMap<const SIZE: usize> {
    dirty_offset: usize,
}

impl<const SIZE: usize> VramMap<SIZE> {
    fn new<const T: usize>(bank: &[u8; T]) -> Self {
        bank.as_ptr().into()
    }

    fn extract_section<const CHUNK_SIZE: usize>(&self, offset: usize) -> VramMap<CHUNK_SIZE> {
        debug_assert!(!self.is_null());
        VramMap::from((self.as_ptr() as usize + CHUNK_SIZE * offset) as *const u8)
    }

    fn as_mut(&mut self) -> VramMapMut<SIZE> {
        debug_assert!(!self.is_null());
        VramMapMut::new(self.as_mut_ptr())
    }

    pub fn is_null(&self) -> bool {
        self.dirty_offset & ((1 << 31) - 1) == ((1 << 31) - 1)
    }

    pub fn as_ptr(&self) -> *const u8 {
        debug_assert!(!self.is_null());
        unsafe { (VRAM_BANK_START + (self.dirty_offset & ((1 << 31) - 1))) as _ }
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.as_ptr() as _
    }

    pub const fn len(&self) -> usize {
        SIZE
    }

    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty_offset &= (1 << 31) - 1;
        self.dirty_offset |= (dirty as usize) << 31;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_offset & (1 << 31) != 0
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

impl<const SIZE: usize> Default for VramMap<SIZE> {
    fn default() -> Self {
        VramMap { dirty_offset: usize::MAX }
    }
}

impl<const SIZE: usize> From<*const u8> for VramMap<SIZE> {
    fn from(value: *const u8) -> Self {
        let offset = unsafe { value as usize - VRAM_BANK_START };
        unsafe { assert_unchecked(offset < TOTAL_SIZE) };
        VramMap { dirty_offset: offset | (1 << 31) }
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

static mut OVERLAP_READ_BUF: [u8; 16 * 1024] = [0; 16 * 1024];

#[derive(Copy, Clone)]
struct OverlapSection<const SIZE: usize, const MAX_OVERLAP: usize> {
    overlaps: [VramMap<SIZE>; MAX_OVERLAP],
    dirty_count: u8,
}

impl<const SIZE: usize, const MAX_OVERLAP: usize> OverlapSection<SIZE, MAX_OVERLAP> {
    fn count(&self) -> usize {
        (self.dirty_count & 0x7F) as usize
    }

    fn set_dirty(&mut self, dirty: bool) {
        self.dirty_count &= 0x7F;
        self.dirty_count |= (dirty as u8) << 7;
    }

    fn is_dirty(&self) -> bool {
        self.dirty_count & 0x80 != 0
    }

    fn add(&mut self, map: VramMap<SIZE>) {
        let count = self.count();
        unsafe { assert_unchecked(count < MAX_OVERLAP) };
        self.overlaps[count] = map;
        self.dirty_count += 1;
        self.set_dirty(true);
    }

    fn get_ptr(&self, index: u32) -> *const u8 {
        let count = self.count();
        unsafe { assert_unchecked(count <= MAX_OVERLAP) };
        if count == 1 {
            unsafe { self.overlaps[0].as_ptr().add(index as usize) }
        } else {
            ptr::null()
        }
    }

    fn read<T: utils::Convert>(&self, index: u32) -> T {
        let count = self.count();
        unsafe { assert_unchecked(count <= MAX_OVERLAP) };
        let mut ret = 0;
        for i in 0..count {
            let map = &self.overlaps[i];
            debug_assert!(!map.is_null());
            ret |= utils::read_from_mem::<T>(map, index).into();
        }
        T::from(ret)
    }

    fn read_all(&mut self, index: u32, buf: &mut [u8; SIZE]) {
        let count = self.count();
        unsafe { assert_unchecked(count <= MAX_OVERLAP) };
        if self.is_dirty() {
            self.set_dirty(false);
            buf.fill(0);
            for i in 0..count {
                let map = &mut self.overlaps[i];
                if !map.is_null() {
                    utils::read_from_mem_slice(map, index, unsafe { &mut OVERLAP_READ_BUF[..SIZE] });
                    for i in 0..SIZE {
                        buf[i] |= unsafe { OVERLAP_READ_BUF[i] };
                    }
                }
            }
        }
    }

    fn write<T: utils::Convert>(&mut self, index: u32, value: T) {
        let count = self.count();
        unsafe { assert_unchecked(count <= MAX_OVERLAP) };
        self.set_dirty(true);
        for i in 0..count {
            let map = &mut self.overlaps[i];
            let mut map = map.as_mut();
            utils::write_to_mem(&mut map, index, value);
        }
    }

    fn write_slice<T: utils::Convert>(&mut self, index: u32, slice: &[T]) {
        let count = self.count();
        unsafe { assert_unchecked(count <= MAX_OVERLAP) };
        self.set_dirty(true);
        for i in 0..count {
            let map = &mut self.overlaps[i];
            let mut map = map.as_mut();
            utils::write_to_mem_slice(&mut map, index as usize, slice);
        }
    }
}

impl<const SIZE: usize, const MAX_OVERLAP: usize> Default for OverlapSection<SIZE, MAX_OVERLAP> {
    fn default() -> Self {
        OverlapSection {
            overlaps: [VramMap::default(); MAX_OVERLAP],
            dirty_count: 1 << 7,
        }
    }
}

struct OverlapMapping<const SIZE: usize, const CHUNK_SIZE: usize, const MAX_OVERLAP: usize>
where
    [(); SIZE / CHUNK_SIZE]:,
{
    sections: [OverlapSection<CHUNK_SIZE, MAX_OVERLAP>; SIZE / CHUNK_SIZE],
}

impl<const SIZE: usize, const CHUNK_SIZE: usize, const MAX_OVERLAP: usize> OverlapMapping<SIZE, CHUNK_SIZE, MAX_OVERLAP>
where
    [(); SIZE / CHUNK_SIZE]:,
{
    const _CHECK: () = [()][(CHUNK_SIZE > 16 * 1024) as usize];

    fn new() -> Self {
        OverlapMapping {
            sections: [OverlapSection::default(); SIZE / CHUNK_SIZE],
        }
    }

    fn reset(&mut self) {
        for s in &mut self.sections {
            s.dirty_count = 1 << 7;
        }
    }

    fn add<const MAP_SIZE: usize>(&mut self, map: VramMap<MAP_SIZE>, offset: usize) {
        for i in 0..(MAP_SIZE / CHUNK_SIZE) {
            self.sections[offset + i].add(map.extract_section::<CHUNK_SIZE>(i))
        }
    }

    fn get_ptr(&self, mut addr: u32) -> *const u8 {
        addr %= SIZE as u32;
        let section_index = addr as usize / CHUNK_SIZE;
        let section_offset = addr as usize % CHUNK_SIZE;
        self.sections[section_index].get_ptr(section_offset as u32)
    }

    fn read<T: utils::Convert>(&self, mut addr: u32) -> T {
        addr %= SIZE as u32;
        let section_index = addr as usize / CHUNK_SIZE;
        let section_offset = addr as usize % CHUNK_SIZE;
        self.sections[section_index].read(section_offset as u32)
    }

    fn read_all(&mut self, mut addr: u32, buf: &mut [u8; SIZE]) {
        addr %= SIZE as u32;
        for chunk_addr in (addr..addr + SIZE as u32).step_by(CHUNK_SIZE) {
            let section_index = chunk_addr as usize / CHUNK_SIZE;
            let section_offset = chunk_addr as usize % CHUNK_SIZE;
            let buf_start = (chunk_addr - addr) as usize;
            let buf_end = buf_start + CHUNK_SIZE;
            let chunk_buf = unsafe { (buf[buf_start..buf_end].as_mut_ptr() as *mut [u8; CHUNK_SIZE]).as_mut_unchecked() };
            self.sections[section_index].read_all(section_offset as u32, chunk_buf);
        }
    }

    fn write<T: utils::Convert>(&mut self, mut addr: u32, value: T) {
        addr %= SIZE as u32;
        let section_index = addr as usize / CHUNK_SIZE;
        let section_offset = addr as usize % CHUNK_SIZE;
        self.sections[section_index].write(section_offset as u32, value);
    }

    fn write_slice<T: utils::Convert>(&mut self, mut addr: u32, slice: &[T]) {
        addr %= SIZE as u32;
        let mut remaining = size_of_val(slice);
        while remaining != 0 {
            let section_index = addr as usize / CHUNK_SIZE;
            let section_offset = addr as usize % CHUNK_SIZE;
            let slice_start = size_of_val(slice) - remaining;
            let to_write = min(remaining, CHUNK_SIZE - section_offset);
            let slice_end = slice_start + to_write;
            let slice_start = slice_start / size_of::<T>();
            let slice_end = slice_end / size_of::<T>();
            unsafe { assert_unchecked(section_index < self.sections.len() && slice_start < slice_end && slice_end <= slice.len()) }
            self.sections[section_index].write_slice(section_offset as u32, &slice[slice_start..slice_end]);
            addr += to_write as u32;
            remaining -= to_write;
        }
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

pub const BANK_A_SIZE: usize = 128 * 1024;
const BANK_B_SIZE: usize = BANK_A_SIZE;
const BANK_C_SIZE: usize = BANK_A_SIZE;
const BANK_D_SIZE: usize = BANK_A_SIZE;
const BANK_E_SIZE: usize = 64 * 1024;
const BANK_F_SIZE: usize = 16 * 1024;
const BANK_G_SIZE: usize = 16 * 1024;
const BANK_H_SIZE: usize = 32 * 1024;
const BANK_I_SIZE: usize = 16 * 1024;
pub const TOTAL_SIZE: usize = BANK_A_SIZE + BANK_B_SIZE + BANK_C_SIZE + BANK_D_SIZE + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE + BANK_H_SIZE + BANK_I_SIZE;
const_assert_eq!(TOTAL_SIZE, 656 * 1024);

struct VramBanks(HeapMemU8<TOTAL_SIZE>);

macro_rules! create_vram_bank {
    ($name:ident, $offset:expr, $size:expr) => {
        paste! {
            fn [<get _ $name>](&self) -> &[u8; $size] {
                const_assert!($offset + $size <= TOTAL_SIZE);
                unsafe {
                    (self.0[$offset..$offset + $size].as_ptr() as *const [u8; $size])
                        .as_ref()
                        .unwrap_unchecked()
                }
            }
        }
    };
}

impl VramBanks {
    fn new() -> Self {
        VramBanks(HeapMemU8::new())
    }

    create_vram_bank!(a, 0, BANK_A_SIZE);
    create_vram_bank!(b, BANK_A_SIZE, BANK_B_SIZE);
    create_vram_bank!(c, BANK_A_SIZE * 2, BANK_C_SIZE);
    create_vram_bank!(d, BANK_A_SIZE * 3, BANK_D_SIZE);
    create_vram_bank!(e, BANK_A_SIZE * 4, BANK_E_SIZE);
    create_vram_bank!(f, BANK_A_SIZE * 4 + BANK_E_SIZE, BANK_F_SIZE);
    create_vram_bank!(g, BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE, BANK_G_SIZE);
    create_vram_bank!(h, BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE, BANK_H_SIZE);
    create_vram_bank!(i, BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE + BANK_H_SIZE, BANK_I_SIZE);
}

pub const BG_A_SIZE: u32 = 512 * 1024;
pub const OBJ_A_SIZE: u32 = 256 * 1024;
pub const BG_B_SIZE: u32 = 128 * 1024;
pub const OBJ_B_SIZE: u32 = 128 * 1024;

pub const BG_EXT_PAL_SIZE: u32 = 32 * 1024;
pub const OBJ_EXT_PAL_SIZE: u32 = 8 * 1024;

pub const TEX_REAR_PLANE_IMAGE_SIZE: u32 = 4 * 128 * 1024;
pub const TEX_PAL_SIZE: u32 = 6 * 16 * 1024;

pub const LCDC_OFFSET: u32 = 0x800000;
pub const BG_A_OFFSET: u32 = 0x000000;
pub const OBJ_A_OFFSET: u32 = 0x400000;
pub const BG_B_OFFSET: u32 = 0x200000;
pub const OBJ_B_OFFSET: u32 = 0x600000;

pub const ARM7_SIZE: u32 = 128 * 1024;

pub struct VramMaps {
    lcdc: OverlapMapping<TOTAL_SIZE, { 16 * 1024 }, 1>,

    bg_a: OverlapMapping<{ BG_A_SIZE as usize }, { 16 * 1024 }, 4>,
    obj_a: OverlapMapping<{ 256 * 1024 }, { 16 * 1024 }, 2>,
    bg_ext_palette_a: [VramMap<{ BG_EXT_PAL_SIZE as usize / 4 }>; 4],
    obj_ext_palette_a: VramMap<{ OBJ_EXT_PAL_SIZE as usize }>,

    tex_rear_plane_img: [VramMap<{ 128 * 1024 }>; 4],
    tex_palette: [VramMap<{ 16 * 1024 }>; 6],

    bg_b: OverlapMapping<{ BG_B_SIZE as usize }, { 16 * 1024 }, 1>,
    obj_b: OverlapMapping<{ 128 * 1024 }, { 16 * 1024 }, 1>,
    bg_ext_palette_b: [VramMap<{ BG_EXT_PAL_SIZE as usize / 4 }>; 4],
    obj_ext_palette_b: VramMap<{ OBJ_EXT_PAL_SIZE as usize }>,
}

impl VramMaps {
    fn new() -> Self {
        VramMaps {
            lcdc: OverlapMapping::new(),

            bg_a: OverlapMapping::new(),
            obj_a: OverlapMapping::new(),
            bg_ext_palette_a: [VramMap::default(); 4],
            obj_ext_palette_a: VramMap::default(),

            tex_rear_plane_img: [VramMap::default(); 4],
            tex_palette: [VramMap::default(); 6],

            bg_b: OverlapMapping::new(),
            obj_b: OverlapMapping::new(),
            bg_ext_palette_b: [VramMap::default(); 4],
            obj_ext_palette_b: VramMap::default(),
        }
    }

    fn reset(&mut self) {
        self.lcdc.reset();
        self.bg_a.reset();
        self.obj_a.reset();
        self.bg_ext_palette_a.fill(VramMap::default());
        self.obj_ext_palette_a = VramMap::default();
        self.tex_rear_plane_img.fill(VramMap::default());
        self.tex_palette.fill(VramMap::default());
        self.bg_b.reset();
        self.obj_b.reset();
        self.bg_ext_palette_b.fill(VramMap::default());
        self.obj_ext_palette_b = VramMap::default();
    }
}

pub struct Vram {
    pub stat: u8,
    pub cnt: [u8; BANK_SIZE],
    banks: VramBanks,
    maps: VramMaps,
    arm7: OverlapMapping<{ 128 * 2 * 1024 }, { ARM7_SIZE as usize }, 2>,
}

impl Vram {
    pub fn new() -> Self {
        let mut banks = VramBanks::new();
        unsafe { VRAM_BANK_START = banks.0.as_mut_ptr() as usize };
        Vram {
            stat: 0,
            cnt: [0u8; BANK_SIZE],
            banks,
            maps: VramMaps::new(),
            arm7: OverlapMapping::new(),
        }
    }

    pub fn set_cnt(&mut self, bank: usize, value: u8, emu: &mut Emu) {
        const MASKS: [u8; 9] = [0x9B, 0x9B, 0x9F, 0x9F, 0x87, 0x9F, 0x9F, 0x83, 0x83];
        let value = value & MASKS[bank];
        if self.cnt[bank] == value {
            return;
        }
        self.cnt[bank] = value;

        debug_println!("Set vram cnt {:x} to {:x}", bank, value);

        self.maps.reset();
        self.arm7.reset();
        self.stat = 0;

        {
            let cnt_a = VramCnt::from(self.cnt[0]);
            if bool::from(cnt_a.enable()) {
                let mst = u8::from(cnt_a.mst()) & 0x3;
                match mst {
                    0 => {
                        let map: VramMap<BANK_A_SIZE> = VramMap::new(self.banks.get_a());
                        self.maps.lcdc.add::<BANK_A_SIZE>(map, 0);
                    }
                    1 => {
                        let ofs = u8::from(cnt_a.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_A_SIZE>(VramMap::new(self.banks.get_a()), 128 / 16 * ofs);
                    }
                    2 => {
                        let ofs = u8::from(cnt_a.ofs()) as usize;
                        self.maps.obj_a.add::<BANK_A_SIZE>(VramMap::new(self.banks.get_a()), 128 / 16 * (ofs & 1));
                    }
                    3 => {
                        let ofs = u8::from(cnt_a.ofs()) as usize;
                        self.maps.tex_rear_plane_img[ofs] = VramMap::new(self.banks.get_a());
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_b = VramCnt::from(self.cnt[1]);
            if bool::from(cnt_b.enable()) {
                let mst = u8::from(cnt_b.mst()) & 0x3;
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_B_SIZE>(VramMap::new(self.banks.get_b()), BANK_A_SIZE / 1024 / 16);
                    }
                    1 => {
                        let ofs = u8::from(cnt_b.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_B_SIZE>(VramMap::new(self.banks.get_b()), 128 / 16 * ofs);
                    }
                    2 => {
                        let ofs = u8::from(cnt_b.ofs()) as usize;
                        self.maps.obj_a.add::<BANK_B_SIZE>(VramMap::new(self.banks.get_b()), 128 / 16 * (ofs & 1));
                    }
                    3 => {
                        let ofs = u8::from(cnt_b.ofs()) as usize;
                        self.maps.tex_rear_plane_img[ofs] = VramMap::new(self.banks.get_b());
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_c = VramCnt::from(self.cnt[2]);
            if bool::from(cnt_c.enable()) {
                let mst = u8::from(cnt_c.mst());
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_C_SIZE>(VramMap::new(self.banks.get_c()), BANK_A_SIZE / 1024 / 16 * 2);
                    }
                    1 => {
                        let ofs = u8::from(cnt_c.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_C_SIZE>(VramMap::new(self.banks.get_c()), 128 / 16 * ofs);
                    }
                    2 => {
                        let ofs = u8::from(cnt_c.ofs()) as usize;
                        self.arm7.add::<BANK_C_SIZE>(VramMap::new(self.banks.get_c()), ofs & 1);
                        self.stat |= 1;
                    }
                    3 => {
                        let ofs = u8::from(cnt_c.ofs()) as usize;
                        self.maps.tex_rear_plane_img[ofs] = VramMap::new(self.banks.get_c());
                    }
                    4 => {
                        self.maps.bg_b.add::<BANK_C_SIZE>(VramMap::new(self.banks.get_c()), 0);
                    }
                    _ => {
                        unreachable!()
                    }
                }
            }
        }

        {
            let cnt_d = VramCnt::from(self.cnt[3]);
            if bool::from(cnt_d.enable()) {
                let mst = u8::from(cnt_d.mst());
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_D_SIZE>(VramMap::new(self.banks.get_d()), BANK_A_SIZE / 1024 / 16 * 3);
                    }
                    1 => {
                        let ofs = u8::from(cnt_d.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_D_SIZE>(VramMap::new(self.banks.get_d()), 128 / 16 * ofs);
                    }
                    2 => {
                        let ofs = u8::from(cnt_d.ofs()) as usize;
                        self.arm7.add::<BANK_D_SIZE>(VramMap::new(self.banks.get_d()), ofs & 1);
                        self.stat |= 2;
                    }
                    3 => {
                        let ofs = u8::from(cnt_d.ofs()) as usize;
                        self.maps.tex_rear_plane_img[ofs] = VramMap::new(self.banks.get_d());
                    }
                    4 => {
                        self.maps.obj_b.add::<BANK_D_SIZE>(VramMap::new(self.banks.get_d()), 0);
                    }
                    _ => {
                        unreachable!()
                    }
                }
            }
        }

        {
            let cnt_e = VramCnt::from(self.cnt[4]);
            if bool::from(cnt_e.enable()) {
                let mst = u8::from(cnt_e.mst());
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_E_SIZE>(VramMap::new(self.banks.get_e()), BANK_A_SIZE / 1024 / 16 * 4);
                    }
                    1 => {
                        self.maps.bg_a.add::<BANK_E_SIZE>(VramMap::new(self.banks.get_e()), 0);
                    }
                    2 => {
                        self.maps.obj_a.add::<BANK_E_SIZE>(VramMap::new(self.banks.get_e()), 0);
                    }
                    3 => {
                        let vram_map = VramMap::<BANK_E_SIZE>::new(self.banks.get_e());
                        for i in 0..4 {
                            self.maps.tex_palette[i] = vram_map.extract_section(i);
                        }
                    }
                    4 => {
                        let vram_map = VramMap::<BANK_E_SIZE>::new(self.banks.get_e());
                        for i in 0..4 {
                            self.maps.bg_ext_palette_a[i] = vram_map.extract_section(i);
                        }
                    }
                    _ => {
                        unreachable!()
                    }
                }
            }
        }

        {
            let cnt_f = VramCnt::from(self.cnt[5]);
            if bool::from(cnt_f.enable()) {
                let mst = u8::from(cnt_f.mst());
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_F_SIZE>(VramMap::new(self.banks.get_f()), (BANK_A_SIZE * 4 + BANK_E_SIZE) / 1024 / 16);
                    }
                    1 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_F_SIZE>(VramMap::new(self.banks.get_f()), (ofs & 1) + 2 * (ofs & 2));
                    }
                    2 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        self.maps.obj_a.add::<BANK_F_SIZE>(VramMap::new(self.banks.get_f()), (ofs & 1) + 2 * (ofs & 2));
                    }
                    3 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        self.maps.tex_palette[(ofs & 1) + ((ofs & 2) * 2)] = VramMap::new(self.banks.get_f());
                    }
                    4 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        let vram_map = VramMap::<BANK_F_SIZE>::new(self.banks.get_f());
                        for i in 0..2 {
                            self.maps.bg_ext_palette_a[(ofs & 1) * 2 + i] = vram_map.extract_section(i);
                        }
                    }
                    5 => {
                        self.maps.obj_ext_palette_a = VramMap::<BANK_F_SIZE>::new(self.banks.get_f()).extract_section(0);
                    }
                    _ => {
                        unreachable!()
                    }
                }
            }
        }

        {
            let cnt_g = VramCnt::from(self.cnt[6]);
            if bool::from(cnt_g.enable()) {
                let mst = u8::from(cnt_g.mst());
                match mst {
                    0 => {
                        self.maps
                            .lcdc
                            .add::<BANK_G_SIZE>(VramMap::new(self.banks.get_g()), (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE) / 1024 / 16);
                    }
                    1 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_G_SIZE>(VramMap::new(self.banks.get_g()), (ofs & 1) + 2 * (ofs & 2));
                    }
                    2 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        self.maps.obj_a.add::<BANK_G_SIZE>(VramMap::new(self.banks.get_g()), (ofs & 1) + 2 * (ofs & 2));
                    }
                    3 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        self.maps.tex_palette[((ofs & 2) << 1) + (ofs & 1)] = VramMap::new(self.banks.get_g())
                    }
                    4 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        let vram_map = VramMap::<BANK_G_SIZE>::new(self.banks.get_g());
                        for i in 0..2 {
                            self.maps.bg_ext_palette_a[(ofs & 1) * 2 + i] = vram_map.extract_section(i);
                        }
                    }
                    5 => {
                        self.maps.obj_ext_palette_a = VramMap::<BANK_G_SIZE>::new(self.banks.get_g()).extract_section(0);
                    }
                    _ => {
                        unreachable!()
                    }
                }
            }
        }

        {
            let cnt_h = VramCnt::from(self.cnt[7]);
            if bool::from(cnt_h.enable()) {
                let mst = u8::from(cnt_h.mst()) & 0x3;
                match mst {
                    0 => {
                        self.maps
                            .lcdc
                            .add::<BANK_H_SIZE>(VramMap::new(self.banks.get_h()), (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE) / 1024 / 16);
                    }
                    1 => {
                        self.maps.bg_b.add::<BANK_H_SIZE>(VramMap::new(self.banks.get_h()), 0);
                    }
                    2 => {
                        let vram_map = VramMap::<BANK_H_SIZE>::new(self.banks.get_h());
                        for i in 0..4 {
                            self.maps.bg_ext_palette_b[i] = vram_map.extract_section(i);
                        }
                    }
                    _ => {
                        unreachable!()
                    }
                }
            }
        }

        {
            let cnt_i = VramCnt::from(self.cnt[8]);
            if bool::from(cnt_i.enable()) {
                let mst = u8::from(cnt_i.mst()) & 0x3;
                match mst {
                    0 => {
                        self.maps
                            .lcdc
                            .add::<BANK_I_SIZE>(VramMap::new(self.banks.get_i()), (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE + BANK_H_SIZE) / 1024 / 16);
                    }
                    1 => {
                        self.maps.bg_b.add::<BANK_I_SIZE>(VramMap::new(self.banks.get_i()), 2);
                    }
                    2 => {
                        self.maps.obj_b.add(VramMap::<BANK_I_SIZE>::new(self.banks.get_i()), 0);
                    }
                    3 => {
                        self.maps.obj_ext_palette_b = VramMap::<BANK_I_SIZE>::new(self.banks.get_i()).extract_section(0);
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        get_jit_mut!(emu).invalidate_vram();
    }

    pub fn get_ptr<const CPU: CpuType>(&self, addr: u32) -> *const u8 {
        let base_addr = addr & 0xF00000;
        let addr_offset = (addr - base_addr) & 0xFFFFF;
        match CPU {
            ARM9 => match addr & 0xF00000 {
                LCDC_OFFSET => self.maps.lcdc.get_ptr(addr_offset),
                BG_A_OFFSET => self.maps.bg_a.get_ptr(addr_offset),
                OBJ_A_OFFSET => self.maps.obj_a.get_ptr(addr_offset),
                BG_B_OFFSET => self.maps.bg_b.get_ptr(addr_offset),
                OBJ_B_OFFSET => self.maps.obj_b.get_ptr(addr_offset),
                _ => ptr::null(),
            },
            ARM7 => self.arm7.get_ptr(addr_offset),
        }
    }

    pub fn read<const CPU: CpuType, T: utils::Convert>(&self, addr: u32) -> T {
        let base_addr = addr & 0xF00000;
        let addr_offset = addr & 0xFFFFF;
        match CPU {
            ARM9 => match base_addr {
                LCDC_OFFSET => self.maps.lcdc.read(addr_offset),
                BG_A_OFFSET => self.maps.bg_a.read(addr_offset),
                OBJ_A_OFFSET => self.maps.obj_a.read(addr_offset),
                BG_B_OFFSET => self.maps.bg_b.read(addr_offset),
                OBJ_B_OFFSET => self.maps.obj_b.read(addr_offset),
                _ => unsafe { unreachable_unchecked() },
            },
            ARM7 => self.arm7.read(addr_offset),
        }
    }

    pub fn write<const CPU: CpuType, T: utils::Convert>(&mut self, addr: u32, value: T) {
        let base_addr = addr & 0xF00000;
        let addr_offset = addr & 0xFFFFF;
        match CPU {
            ARM9 => match base_addr {
                LCDC_OFFSET => self.maps.lcdc.write(addr_offset, value),
                BG_A_OFFSET => self.maps.bg_a.write(addr_offset, value),
                OBJ_A_OFFSET => self.maps.obj_a.write(addr_offset, value),
                BG_B_OFFSET => self.maps.bg_b.write(addr_offset, value),
                OBJ_B_OFFSET => self.maps.obj_b.write(addr_offset, value),
                _ => unsafe { unreachable_unchecked() },
            },
            ARM7 => self.arm7.write(addr_offset, value),
        };
    }

    pub fn write_slice<const CPU: CpuType, T: utils::Convert>(&mut self, addr: u32, slice: &[T]) {
        let base_addr = addr & 0xF00000;
        let addr_offset = addr & 0xFFFFF;
        match CPU {
            ARM9 => match base_addr {
                LCDC_OFFSET => self.maps.lcdc.write_slice(addr_offset, slice),
                BG_A_OFFSET => self.maps.bg_a.write_slice(addr_offset, slice),
                OBJ_A_OFFSET => self.maps.obj_a.write_slice(addr_offset, slice),
                BG_B_OFFSET => self.maps.bg_b.write_slice(addr_offset, slice),
                OBJ_B_OFFSET => self.maps.obj_b.write_slice(addr_offset, slice),
                _ => unsafe { unreachable_unchecked() },
            },
            ARM7 => self.arm7.write_slice(addr_offset, slice),
        };
    }

    pub fn read_all_lcdc(&mut self, buf: &mut [u8; TOTAL_SIZE]) {
        self.maps.lcdc.read_all(0, buf)
    }

    pub fn read_all_bg_a(&mut self, buf: &mut [u8; BG_A_SIZE as usize]) {
        self.maps.bg_a.read_all(0, buf)
    }

    pub fn read_all_obj_a(&mut self, buf: &mut [u8; OBJ_A_SIZE as usize]) {
        self.maps.obj_a.read_all(0, buf)
    }

    pub fn read_all_bg_a_ext_palette(&mut self, buf: &mut [u8; BG_EXT_PAL_SIZE as usize]) {
        for i in 0..self.maps.bg_ext_palette_a.len() {
            let map = &mut self.maps.bg_ext_palette_a[i];
            if map.is_dirty() {
                let buf = &mut buf[i << 13..(i << 13) + 8 * 1024];
                if !map.is_null() {
                    buf.copy_from_slice(map);
                } else {
                    buf.fill(0);
                }
                map.set_dirty(false);
            }
        }
    }

    pub fn read_all_obj_a_ext_palette(&mut self, buf: &mut [u8; OBJ_EXT_PAL_SIZE as usize]) {
        if self.maps.obj_ext_palette_a.is_dirty() {
            if !self.maps.obj_ext_palette_a.is_null() {
                buf.copy_from_slice(&self.maps.obj_ext_palette_a);
            } else {
                buf.fill(0);
            }
            self.maps.obj_ext_palette_a.set_dirty(false);
        }
    }

    pub fn read_bg_b(&mut self, buf: &mut [u8; BG_B_SIZE as usize]) {
        self.maps.bg_b.read_all(0, buf)
    }

    pub fn read_all_obj_b(&mut self, buf: &mut [u8; OBJ_B_SIZE as usize]) {
        self.maps.obj_b.read_all(0, buf)
    }

    pub fn read_all_bg_b_ext_palette(&mut self, buf: &mut [u8; BG_EXT_PAL_SIZE as usize]) {
        for i in 0..self.maps.bg_ext_palette_b.len() {
            let map = &mut self.maps.bg_ext_palette_b[i];
            if map.is_dirty() {
                let buf = &mut buf[i << 13..(i << 13) + 8 * 1024];
                if !map.is_null() {
                    buf.copy_from_slice(map);
                } else {
                    buf.fill(0);
                }
                map.set_dirty(false);
            }
        }
    }

    pub fn read_all_obj_b_ext_palette(&mut self, buf: &mut [u8; OBJ_EXT_PAL_SIZE as usize]) {
        if self.maps.obj_ext_palette_b.is_dirty() {
            if !self.maps.obj_ext_palette_b.is_null() {
                buf.copy_from_slice(&self.maps.obj_ext_palette_b);
            } else {
                buf.fill(0);
            }
            self.maps.obj_ext_palette_b.set_dirty(false);
        }
    }

    pub fn read_all_tex_rear_plane_img(&mut self, buf: &mut [u8; TEX_REAR_PLANE_IMAGE_SIZE as usize]) {
        for i in 0..self.maps.tex_rear_plane_img.len() {
            let map = &mut self.maps.tex_rear_plane_img[i];
            if map.is_dirty() {
                let buf = &mut buf[i << 17..(i << 17) + 128 * 1024];
                if !map.is_null() {
                    buf.copy_from_slice(map);
                } else {
                    buf.fill(0);
                }
                map.set_dirty(false);
            }
        }
    }

    pub fn read_all_tex_palette(&mut self, buf: &mut [u8; TEX_PAL_SIZE as usize]) {
        for i in 0..self.maps.tex_palette.len() {
            let map = &mut self.maps.tex_palette[i];
            if map.is_dirty() {
                let buf = &mut buf[i << 14..(i << 14) + 16 * 1024];
                if !map.is_null() {
                    buf.copy_from_slice(map);
                } else {
                    buf.fill(0);
                }
                map.set_dirty(false);
            }
        }
    }
}

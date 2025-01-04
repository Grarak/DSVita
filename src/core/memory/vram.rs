use crate::core::emu::{get_jit_mut, Emu};
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::logging::debug_println;
use crate::utils;
use crate::utils::HeapMemU8;
use bilge::prelude::*;
use paste::paste;
use static_assertions::{const_assert, const_assert_eq};
use std::cell::UnsafeCell;
use std::cmp::min;
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::ops::{Deref, DerefMut};
use std::{array, mem, ptr, slice};

const BANK_SIZE: usize = 9;

#[derive(Copy, Clone)]
struct VramMap<const SIZE: usize> {
    ptr: *const u8,
    dirty: bool,
}

impl<const SIZE: usize> VramMap<SIZE> {
    fn new<const T: usize>(bank: &[u8; T]) -> Self {
        VramMap { ptr: bank.as_ptr() as _, dirty: true }
    }

    fn extract_section<const CHUNK_SIZE: usize>(&self, offset: usize) -> VramMap<CHUNK_SIZE> {
        debug_assert_ne!(self.ptr, ptr::null_mut());
        VramMap::from((self.ptr as usize + CHUNK_SIZE * offset) as *const u8)
    }

    fn as_mut(&mut self) -> VramMapMut<SIZE> {
        debug_assert_ne!(self.ptr, ptr::null_mut());
        VramMapMut::new(self.ptr as _)
    }

    pub fn as_ptr(&self) -> *const u8 {
        debug_assert_ne!(self.ptr, ptr::null_mut());
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

impl<const SIZE: usize> Default for VramMap<SIZE> {
    fn default() -> Self {
        VramMap { ptr: ptr::null_mut(), dirty: true }
    }
}

impl<const SIZE: usize> From<*const u8> for VramMap<SIZE> {
    fn from(value: *const u8) -> Self {
        VramMap { ptr: value, dirty: true }
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

struct OverlapSection<const SIZE: usize> {
    overlaps: [VramMap<SIZE>; 4],
    count: u8,
    read_buf: UnsafeCell<HeapMemU8<SIZE>>,
}

impl<const SIZE: usize> OverlapSection<SIZE> {
    fn add(&mut self, map: VramMap<SIZE>) {
        unsafe { *self.overlaps.get_unchecked_mut(self.count as usize) = map };
        self.count += 1;
    }

    fn get_ptr(&self, index: u32) -> *const u8 {
        if self.count == 1 {
            unsafe { self.overlaps[0].ptr.add(index as usize) }
        } else {
            ptr::null()
        }
    }

    fn read<T: utils::Convert>(&self, index: u32) -> T {
        let mut ret = 0;
        for i in 0..self.count {
            let map = unsafe { self.overlaps.get_unchecked(i as usize) };
            debug_assert_ne!(map.ptr, ptr::null());
            ret |= utils::read_from_mem::<T>(map, index).into();
        }
        T::from(ret)
    }

    fn read_all(&mut self, index: u32, buf: &mut [u8; SIZE]) {
        let dirty = self.overlaps.iter().any(|map| map.dirty);
        if dirty {
            buf.fill(0);
            let read_buf = unsafe { self.read_buf.get().as_mut().unwrap_unchecked() };
            for i in 0..self.count {
                let map = unsafe { self.overlaps.get_unchecked_mut(i as usize) };
                map.dirty = false;
                if !map.ptr.is_null() {
                    utils::read_from_mem_slice(map, index, read_buf.deref_mut());
                    for i in 0..SIZE {
                        buf[i] |= read_buf[i];
                    }
                }
            }
        }
    }

    fn write<T: utils::Convert>(&mut self, index: u32, value: T) {
        for i in 0..self.count {
            let map = unsafe { self.overlaps.get_unchecked_mut(i as usize) };
            map.dirty = true;
            let mut map = map.as_mut();
            debug_assert_ne!(map.ptr, ptr::null_mut());
            utils::write_to_mem(&mut map, index, value);
        }
    }

    fn write_slice<T: utils::Convert>(&mut self, index: u32, slice: &[T]) {
        for i in 0..self.count {
            let map = unsafe { self.overlaps.get_unchecked_mut(i as usize) };
            map.dirty = true;
            let mut map = map.as_mut();
            debug_assert_ne!(map.ptr, ptr::null_mut());
            utils::write_to_mem_slice(&mut map, index as usize, slice);
        }
    }
}

impl<const SIZE: usize> Default for OverlapSection<SIZE> {
    fn default() -> Self {
        OverlapSection {
            overlaps: unsafe { mem::zeroed() },
            count: 0,
            read_buf: UnsafeCell::new(HeapMemU8::new()),
        }
    }
}

struct OverlapMapping<const SIZE: usize, const CHUNK_SIZE: usize>
where
    [(); SIZE / CHUNK_SIZE]:,
{
    sections: [OverlapSection<CHUNK_SIZE>; SIZE / CHUNK_SIZE],
}

impl<const SIZE: usize, const CHUNK_SIZE: usize> OverlapMapping<SIZE, CHUNK_SIZE>
where
    [(); SIZE / CHUNK_SIZE]:,
{
    fn new() -> Self {
        OverlapMapping {
            sections: array::from_fn(|_| OverlapSection::default()),
        }
    }

    fn reset(&mut self) {
        for s in &mut self.sections {
            s.count = 0;
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
            let chunk_buf = unsafe { (buf[buf_start..buf_end].as_mut_ptr() as *mut [u8; CHUNK_SIZE]).as_mut().unwrap_unchecked() };
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

pub struct Vram {
    pub stat: u8,
    pub cnt: [u8; BANK_SIZE],
    banks: VramBanks,

    lcdc: OverlapMapping<TOTAL_SIZE, { 16 * 1024 }>,

    bg_a: OverlapMapping<{ BG_A_SIZE as usize }, { 16 * 1024 }>,
    obj_a: OverlapMapping<{ 256 * 1024 }, { 16 * 1024 }>,
    bg_ext_palette_a: [VramMap<{ BG_EXT_PAL_SIZE as usize / 4 }>; 4],
    obj_ext_palette_a: VramMap<{ OBJ_EXT_PAL_SIZE as usize }>,

    tex_rear_plane_img: [VramMap<{ 128 * 1024 }>; 4],
    tex_palette: [VramMap<{ 16 * 1024 }>; 6],

    bg_b: OverlapMapping<{ BG_B_SIZE as usize }, { 16 * 1024 }>,
    obj_b: OverlapMapping<{ 128 * 1024 }, { 16 * 1024 }>,
    bg_ext_palette_b: [VramMap<{ BG_EXT_PAL_SIZE as usize / 4 }>; 4],
    obj_ext_palette_b: VramMap<{ OBJ_EXT_PAL_SIZE as usize }>,

    arm7: OverlapMapping<{ 128 * 2 * 1024 }, { ARM7_SIZE as usize }>,
}

impl Vram {
    pub fn new() -> Self {
        Vram {
            stat: 0,
            cnt: [0u8; BANK_SIZE],
            banks: VramBanks::new(),

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
        self.arm7.reset();
        self.stat = 0;

        {
            let cnt_a = VramCnt::from(self.cnt[0]);
            if bool::from(cnt_a.enable()) {
                let mst = u8::from(cnt_a.mst()) & 0x3;
                match mst {
                    0 => {
                        let map: VramMap<BANK_A_SIZE> = VramMap::new(self.banks.get_a());
                        self.lcdc.add::<BANK_A_SIZE>(map, 0);
                    }
                    1 => {
                        let ofs = u8::from(cnt_a.ofs()) as usize;
                        self.bg_a.add::<BANK_A_SIZE>(VramMap::new(self.banks.get_a()), 128 / 16 * ofs);
                    }
                    2 => {
                        let ofs = u8::from(cnt_a.ofs()) as usize;
                        self.obj_a.add::<BANK_A_SIZE>(VramMap::new(self.banks.get_a()), 128 / 16 * (ofs & 1));
                    }
                    3 => {
                        let ofs = u8::from(cnt_a.ofs()) as usize;
                        self.tex_rear_plane_img[ofs] = VramMap::new(self.banks.get_a());
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
                        self.lcdc.add::<BANK_B_SIZE>(VramMap::new(self.banks.get_b()), BANK_A_SIZE / 1024 / 16);
                    }
                    1 => {
                        let ofs = u8::from(cnt_b.ofs()) as usize;
                        self.bg_a.add::<BANK_B_SIZE>(VramMap::new(self.banks.get_b()), 128 / 16 * ofs);
                    }
                    2 => {
                        let ofs = u8::from(cnt_b.ofs()) as usize;
                        self.obj_a.add::<BANK_B_SIZE>(VramMap::new(self.banks.get_b()), 128 / 16 * (ofs & 1));
                    }
                    3 => {
                        let ofs = u8::from(cnt_b.ofs()) as usize;
                        self.tex_rear_plane_img[ofs] = VramMap::new(self.banks.get_b());
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
                        self.lcdc.add::<BANK_C_SIZE>(VramMap::new(self.banks.get_c()), BANK_A_SIZE / 1024 / 16 * 2);
                    }
                    1 => {
                        let ofs = u8::from(cnt_c.ofs()) as usize;
                        self.bg_a.add::<BANK_C_SIZE>(VramMap::new(self.banks.get_c()), 128 / 16 * ofs);
                    }
                    2 => {
                        let ofs = u8::from(cnt_c.ofs()) as usize;
                        self.arm7.add::<BANK_C_SIZE>(VramMap::new(self.banks.get_c()), ofs & 1);
                        self.stat |= 1;
                    }
                    3 => {
                        let ofs = u8::from(cnt_c.ofs()) as usize;
                        self.tex_rear_plane_img[ofs] = VramMap::new(self.banks.get_c());
                    }
                    4 => {
                        self.bg_b.add::<BANK_C_SIZE>(VramMap::new(self.banks.get_c()), 0);
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
                        self.lcdc.add::<BANK_D_SIZE>(VramMap::new(self.banks.get_d()), BANK_A_SIZE / 1024 / 16 * 3);
                    }
                    1 => {
                        let ofs = u8::from(cnt_d.ofs()) as usize;
                        self.bg_a.add::<BANK_D_SIZE>(VramMap::new(self.banks.get_d()), 128 / 16 * ofs);
                    }
                    2 => {
                        let ofs = u8::from(cnt_d.ofs()) as usize;
                        self.arm7.add::<BANK_D_SIZE>(VramMap::new(self.banks.get_d()), ofs & 1);
                        self.stat |= 2;
                    }
                    3 => {
                        let ofs = u8::from(cnt_d.ofs()) as usize;
                        self.tex_rear_plane_img[ofs] = VramMap::new(self.banks.get_d());
                    }
                    4 => {
                        self.obj_b.add::<BANK_D_SIZE>(VramMap::new(self.banks.get_d()), 0);
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
                        self.lcdc.add::<BANK_E_SIZE>(VramMap::new(self.banks.get_e()), BANK_A_SIZE / 1024 / 16 * 4);
                    }
                    1 => {
                        self.bg_a.add::<BANK_E_SIZE>(VramMap::new(self.banks.get_e()), 0);
                    }
                    2 => {
                        self.obj_a.add::<BANK_E_SIZE>(VramMap::new(self.banks.get_e()), 0);
                    }
                    3 => {
                        let vram_map = VramMap::<BANK_E_SIZE>::new(self.banks.get_e());
                        for i in 0..4 {
                            self.tex_palette[i] = vram_map.extract_section(i);
                        }
                    }
                    4 => {
                        let vram_map = VramMap::<BANK_E_SIZE>::new(self.banks.get_e());
                        for i in 0..4 {
                            self.bg_ext_palette_a[i] = vram_map.extract_section(i);
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
                        self.lcdc.add::<BANK_F_SIZE>(VramMap::new(self.banks.get_f()), (BANK_A_SIZE * 4 + BANK_E_SIZE) / 1024 / 16);
                    }
                    1 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        self.bg_a.add::<BANK_F_SIZE>(VramMap::new(self.banks.get_f()), (ofs & 1) + 2 * (ofs & 2));
                    }
                    2 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        self.obj_a.add::<BANK_F_SIZE>(VramMap::new(self.banks.get_f()), (ofs & 1) + 2 * (ofs & 2));
                    }
                    3 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        self.tex_palette[(ofs & 1) + ((ofs & 2) * 2)] = VramMap::new(self.banks.get_f());
                    }
                    4 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        let vram_map = VramMap::<BANK_F_SIZE>::new(self.banks.get_f());
                        for i in 0..2 {
                            self.bg_ext_palette_a[(ofs & 1) * 2 + i] = vram_map.extract_section(i);
                        }
                    }
                    5 => {
                        self.obj_ext_palette_a = VramMap::<BANK_F_SIZE>::new(self.banks.get_f()).extract_section(0);
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
                        self.lcdc
                            .add::<BANK_G_SIZE>(VramMap::new(self.banks.get_g()), (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE) / 1024 / 16);
                    }
                    1 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        self.bg_a.add::<BANK_G_SIZE>(VramMap::new(self.banks.get_g()), (ofs & 1) + 2 * (ofs & 2));
                    }
                    2 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        self.obj_a.add::<BANK_G_SIZE>(VramMap::new(self.banks.get_g()), (ofs & 1) + 2 * (ofs & 2));
                    }
                    3 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        self.tex_palette[((ofs & 2) << 1) + (ofs & 1)] = VramMap::new(self.banks.get_g())
                    }
                    4 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        let vram_map = VramMap::<BANK_G_SIZE>::new(self.banks.get_g());
                        for i in 0..2 {
                            self.bg_ext_palette_a[(ofs & 1) * 2 + i] = vram_map.extract_section(i);
                        }
                    }
                    5 => {
                        self.obj_ext_palette_a = VramMap::<BANK_G_SIZE>::new(self.banks.get_g()).extract_section(0);
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
                        self.lcdc
                            .add::<BANK_H_SIZE>(VramMap::new(self.banks.get_h()), (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE) / 1024 / 16);
                    }
                    1 => {
                        self.bg_b.add::<BANK_H_SIZE>(VramMap::new(self.banks.get_h()), 0);
                    }
                    2 => {
                        let vram_map = VramMap::<BANK_H_SIZE>::new(self.banks.get_h());
                        for i in 0..4 {
                            self.bg_ext_palette_b[i] = vram_map.extract_section(i);
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
                        self.lcdc
                            .add::<BANK_I_SIZE>(VramMap::new(self.banks.get_i()), (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE + BANK_H_SIZE) / 1024 / 16);
                    }
                    1 => {
                        self.bg_b.add::<BANK_I_SIZE>(VramMap::new(self.banks.get_i()), 2);
                    }
                    2 => {
                        self.obj_b.add(VramMap::<BANK_I_SIZE>::new(self.banks.get_i()), 0);
                    }
                    3 => {
                        self.obj_ext_palette_b = VramMap::<BANK_I_SIZE>::new(self.banks.get_i()).extract_section(0);
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
                LCDC_OFFSET => self.lcdc.get_ptr(addr_offset),
                BG_A_OFFSET => self.bg_a.get_ptr(addr_offset),
                OBJ_A_OFFSET => self.obj_a.get_ptr(addr_offset),
                BG_B_OFFSET => self.bg_b.get_ptr(addr_offset),
                OBJ_B_OFFSET => self.obj_b.get_ptr(addr_offset),
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
                LCDC_OFFSET => self.lcdc.read(addr_offset),
                BG_A_OFFSET => self.bg_a.read(addr_offset),
                OBJ_A_OFFSET => self.obj_a.read(addr_offset),
                BG_B_OFFSET => self.bg_b.read(addr_offset),
                OBJ_B_OFFSET => self.obj_b.read(addr_offset),
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
                LCDC_OFFSET => self.lcdc.write(addr_offset, value),
                BG_A_OFFSET => self.bg_a.write(addr_offset, value),
                OBJ_A_OFFSET => self.obj_a.write(addr_offset, value),
                BG_B_OFFSET => self.bg_b.write(addr_offset, value),
                OBJ_B_OFFSET => self.obj_b.write(addr_offset, value),
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
                LCDC_OFFSET => self.lcdc.write_slice(addr_offset, slice),
                BG_A_OFFSET => self.bg_a.write_slice(addr_offset, slice),
                OBJ_A_OFFSET => self.obj_a.write_slice(addr_offset, slice),
                BG_B_OFFSET => self.bg_b.write_slice(addr_offset, slice),
                OBJ_B_OFFSET => self.obj_b.write_slice(addr_offset, slice),
                _ => unsafe { unreachable_unchecked() },
            },
            ARM7 => self.arm7.write_slice(addr_offset, slice),
        };
    }

    pub fn read_all_lcdc(&mut self, buf: &mut [u8; TOTAL_SIZE]) {
        self.lcdc.read_all(0, buf)
    }

    pub fn read_all_bg_a(&mut self, buf: &mut [u8; BG_A_SIZE as usize]) {
        self.bg_a.read_all(0, buf)
    }

    pub fn read_all_obj_a(&mut self, buf: &mut [u8; OBJ_A_SIZE as usize]) {
        self.obj_a.read_all(0, buf)
    }

    pub fn read_all_bg_a_ext_palette(&mut self, buf: &mut [u8; BG_EXT_PAL_SIZE as usize]) {
        for i in 0..self.bg_ext_palette_a.len() {
            let map = &mut self.bg_ext_palette_a[i];
            if map.dirty {
                let buf = &mut buf[i << 13..(i << 13) + 8 * 1024];
                if !map.ptr.is_null() {
                    buf.copy_from_slice(map);
                } else {
                    buf.fill(0);
                }
                map.dirty = false;
            }
        }
    }

    pub fn read_all_obj_a_ext_palette(&mut self, buf: &mut [u8; OBJ_EXT_PAL_SIZE as usize]) {
        if self.obj_ext_palette_a.dirty {
            if !self.obj_ext_palette_a.ptr.is_null() {
                buf.copy_from_slice(&self.obj_ext_palette_a);
            } else {
                buf.fill(0);
            }
            self.obj_ext_palette_a.dirty = false;
        }
    }

    pub fn read_bg_b(&mut self, buf: &mut [u8; BG_B_SIZE as usize]) {
        self.bg_b.read_all(0, buf)
    }

    pub fn read_all_obj_b(&mut self, buf: &mut [u8; OBJ_B_SIZE as usize]) {
        self.obj_b.read_all(0, buf)
    }

    pub fn read_all_bg_b_ext_palette(&mut self, buf: &mut [u8; BG_EXT_PAL_SIZE as usize]) {
        for i in 0..self.bg_ext_palette_b.len() {
            let map = &mut self.bg_ext_palette_b[i];
            if map.dirty {
                let buf = &mut buf[i << 13..(i << 13) + 8 * 1024];
                if !map.ptr.is_null() {
                    buf.copy_from_slice(map);
                } else {
                    buf.fill(0);
                }
                map.dirty = false;
            }
        }
    }

    pub fn read_all_obj_b_ext_palette(&mut self, buf: &mut [u8; OBJ_EXT_PAL_SIZE as usize]) {
        if self.obj_ext_palette_b.dirty {
            if !self.obj_ext_palette_b.ptr.is_null() {
                buf.copy_from_slice(&self.obj_ext_palette_b);
            } else {
                buf.fill(0);
            }
            self.obj_ext_palette_b.dirty = false;
        }
    }

    pub fn read_all_tex_rear_plane_img(&mut self, buf: &mut [u8; TEX_REAR_PLANE_IMAGE_SIZE as usize]) {
        for i in 0..self.tex_rear_plane_img.len() {
            let map = &mut self.tex_rear_plane_img[i];
            if map.dirty {
                let buf = &mut buf[i << 17..(i << 17) + 128 * 1024];
                if !map.ptr.is_null() {
                    buf.copy_from_slice(map);
                } else {
                    buf.fill(0);
                }
                map.dirty = false;
            }
        }
    }

    pub fn read_all_tex_palette(&mut self, buf: &mut [u8; TEX_PAL_SIZE as usize]) {
        for i in 0..self.tex_palette.len() {
            let map = &mut self.tex_palette[i];
            if map.dirty {
                let buf = &mut buf[i << 14..(i << 14) + 16 * 1024];
                if !map.ptr.is_null() {
                    buf.copy_from_slice(map);
                } else {
                    buf.fill(0);
                }
                map.dirty = false;
            }
        }
    }
}

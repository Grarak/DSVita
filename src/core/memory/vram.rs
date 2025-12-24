use crate::core::emu::Emu;
use crate::core::memory::regions;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::logging::debug_println;
use crate::utils;
use bilge::prelude::*;
use paste::paste;
use static_assertions::{const_assert, const_assert_eq};
use std::cmp::min;
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::intrinsics::{likely, unlikely};
use std::mem;

const BANK_SECTION_SHIFT: usize = 12;
const BANK_SECTION_SIZE: usize = 1 << BANK_SECTION_SHIFT;
pub const BANK_SIZE: usize = 9;

#[derive(Copy, Clone)]
struct VramMap<const SIZE: usize> {
    offset: usize,
}

impl<const SIZE: usize> VramMap<SIZE> {
    fn new(offset: usize) -> Self {
        VramMap { offset }
    }

    fn extract_section<const CHUNK_SIZE: usize>(&self, offset: usize) -> VramMap<CHUNK_SIZE> {
        debug_assert!(!self.is_null() && CHUNK_SIZE * offset < SIZE);
        VramMap {
            offset: self.offset + CHUNK_SIZE * offset,
        }
    }

    pub fn is_null(&self) -> bool {
        self.offset == usize::MAX
    }

    pub fn as_ptr(&self, vram_bank_start: *const u8) -> *const u8 {
        debug_assert!(!self.is_null());
        unsafe { vram_bank_start.add(self.offset) }
    }

    pub fn as_mut_ptr(&mut self, vram_bank_start: *const u8) -> *mut u8 {
        self.as_ptr(vram_bank_start) as _
    }

    pub const fn len(&self) -> usize {
        SIZE
    }

    pub fn as_ref(&self, vram: &[u8; TOTAL_SIZE]) -> &[u8; SIZE] {
        unsafe { (vram.as_ptr().add(self.offset) as *const [u8; SIZE]).as_ref_unchecked() }
    }

    pub fn as_mut(&mut self, vram: &[u8; TOTAL_SIZE]) -> &mut [u8; SIZE] {
        unsafe { (vram.as_ptr().add(self.offset) as *mut [u8; SIZE]).as_mut_unchecked() }
    }
}

impl<const SIZE: usize> Default for VramMap<SIZE> {
    fn default() -> Self {
        VramMap { offset: usize::MAX }
    }
}

static mut OVERLAP_READ_BUF: [u8; 16 * 1024] = [0; 16 * 1024];

#[derive(Copy, Clone)]
struct OverlapSection<const SIZE: usize, const MAX_OVERLAP: usize> {
    overlaps: [VramMap<SIZE>; MAX_OVERLAP],
    count: u8,
}

impl<const SIZE: usize, const MAX_OVERLAP: usize> OverlapSection<SIZE, MAX_OVERLAP> {
    fn add(&mut self, map: VramMap<SIZE>) {
        unsafe { assert_unchecked((self.count as usize) < MAX_OVERLAP) };
        self.overlaps[self.count as usize] = map;
        self.count += 1;
    }

    fn read<T: utils::Convert>(&self, index: u32, vram: &[u8; TOTAL_SIZE]) -> T {
        unsafe { assert_unchecked((self.count as usize) <= MAX_OVERLAP) };
        let mut ret = 0;
        for i in 0..self.count as usize {
            let map = &self.overlaps[i];
            debug_assert!(!map.is_null());
            ret |= utils::read_from_mem::<T>(map.as_ref(vram), index).into();
        }
        T::from(ret)
    }

    fn read_all(&self, index: u32, buf: &mut [u8; SIZE], vram: &[u8; TOTAL_SIZE]) {
        unsafe { assert_unchecked((self.count as usize) <= MAX_OVERLAP) };
        if likely(self.count == 1) {
            let map = &self.overlaps[0];
            if !map.is_null() {
                utils::read_from_mem_slice(map.as_ref(vram), index, buf);
            } else {
                buf.fill(0);
            }
        } else {
            buf.fill(0);
            for i in 0..self.count as usize {
                let map = &self.overlaps[i];
                if !map.is_null() {
                    utils::read_from_mem_slice(map.as_ref(vram), index, unsafe { &mut OVERLAP_READ_BUF[..SIZE] });
                    for i in 0..SIZE {
                        buf[i] |= unsafe { OVERLAP_READ_BUF[i] };
                    }
                }
            }
        }
    }

    fn write<T: utils::Convert>(&mut self, index: u32, value: T, vram: &mut [u8; TOTAL_SIZE]) {
        unsafe { assert_unchecked((self.count as usize) <= MAX_OVERLAP) };
        for i in 0..self.count as usize {
            let map = &mut self.overlaps[i];
            utils::write_to_mem(map.as_mut(vram), index, value);
        }
    }

    fn write_slice<T: utils::Convert>(&mut self, index: u32, slice: &[T], vram: &mut [u8; TOTAL_SIZE]) {
        unsafe { assert_unchecked((self.count as usize) <= MAX_OVERLAP) };
        for i in 0..self.count as usize {
            let map = &mut self.overlaps[i];
            utils::write_to_mem_slice(map.as_mut(vram), index as usize, slice);
        }
    }
}

impl<const SIZE: usize, const MAX_OVERLAP: usize> Default for OverlapSection<SIZE, MAX_OVERLAP> {
    fn default() -> Self {
        OverlapSection {
            overlaps: [VramMap::default(); MAX_OVERLAP],
            count: 0,
        }
    }
}

#[derive(Clone)]
struct OverlapMapping<const BASE_ADDR: u32, const SIZE: usize, const CHUNK_SIZE: usize, const MAX_OVERLAP: usize>
where
    [(); SIZE / CHUNK_SIZE]:,
{
    sections: [OverlapSection<CHUNK_SIZE, MAX_OVERLAP>; SIZE / CHUNK_SIZE],
}

impl<const BASE_ADDR: u32, const SIZE: usize, const CHUNK_SIZE: usize, const MAX_OVERLAP: usize> OverlapMapping<BASE_ADDR, SIZE, CHUNK_SIZE, MAX_OVERLAP>
where
    [(); SIZE / CHUNK_SIZE]:,
{
    const _CHECK: () = [()][(CHUNK_SIZE > 16 * 1024) as usize];

    fn reset(&mut self) {
        for s in &mut self.sections {
            s.count = 0;
        }
    }

    fn add<const MAP_SIZE: usize>(&mut self, map: VramMap<MAP_SIZE>, offset: usize, map_addrs: &mut (u32, usize)) {
        for i in 0..(MAP_SIZE / CHUNK_SIZE) {
            self.sections[offset + i].add(map.extract_section::<CHUNK_SIZE>(i))
        }
        *map_addrs = (BASE_ADDR + 16 * 1024, MAP_SIZE);
    }

    fn read<T: utils::Convert>(&self, mut addr: u32, vram: &[u8; TOTAL_SIZE]) -> T {
        addr %= SIZE as u32;
        let section_index = addr as usize / CHUNK_SIZE;
        let section_offset = addr as usize % CHUNK_SIZE;
        self.sections[section_index].read(section_offset as u32, vram)
    }

    fn read_all(&self, mut addr: u32, buf: &mut [u8; SIZE], vram: &[u8; TOTAL_SIZE]) {
        addr %= SIZE as u32;
        for chunk_addr in (addr..addr + SIZE as u32).step_by(CHUNK_SIZE) {
            let section_index = chunk_addr as usize / CHUNK_SIZE;
            let section_offset = chunk_addr as usize % CHUNK_SIZE;
            let buf_start = (chunk_addr - addr) as usize;
            let buf_end = buf_start + CHUNK_SIZE;
            let chunk_buf = unsafe { (buf[buf_start..buf_end].as_mut_ptr() as *mut [u8; CHUNK_SIZE]).as_mut_unchecked() };
            self.sections[section_index].read_all(section_offset as u32, chunk_buf, vram);
        }
    }

    fn write<T: utils::Convert>(&mut self, mut addr: u32, value: T, vram: &mut [u8; TOTAL_SIZE]) {
        addr %= SIZE as u32;
        let section_index = addr as usize / CHUNK_SIZE;
        let section_offset = addr as usize % CHUNK_SIZE;
        self.sections[section_index].write(section_offset as u32, value, vram);
    }

    fn write_slice<T: utils::Convert>(&mut self, mut addr: u32, slice: &[T], vram: &mut [u8; TOTAL_SIZE]) {
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
            self.sections[section_index].write_slice(section_offset as u32, &slice[slice_start..slice_end], vram);
            addr += to_write as u32;
            remaining -= to_write;
        }
    }

    pub fn get_offset(&self, mut addr: u32) -> usize {
        addr %= SIZE as u32;
        let section_index = addr as usize / CHUNK_SIZE;
        let section_offset = addr as usize % CHUNK_SIZE;
        if self.sections[section_index].count == 1 {
            self.sections[section_index].overlaps[0].offset + section_offset
        } else {
            0
        }
    }
}

impl<const BASE_ADDR: u32, const SIZE: usize, const CHUNK_SIZE: usize, const MAX_OVERLAP: usize> Default for OverlapMapping<BASE_ADDR, SIZE, CHUNK_SIZE, MAX_OVERLAP>
where
    [(); SIZE / CHUNK_SIZE]:,
{
    fn default() -> Self {
        OverlapMapping {
            sections: [OverlapSection::default(); SIZE / CHUNK_SIZE],
        }
    }
}

#[bitsize(8)]
#[derive(FromBits)]
pub struct VramCnt {
    pub mst: u3,
    ofs: u2,
    not_used: u2,
    pub enable: bool,
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

#[derive(Default)]
pub struct VramBanks;

macro_rules! create_vram_bank {
    ($name:ident, $offset:expr, $size:expr) => {
        paste! {
            const fn [<get _ $name>]() -> usize {
                const_assert!($offset + $size <= TOTAL_SIZE);
                $offset
            }
        }
    };
}

impl VramBanks {
    pub fn copy_bank(bank: u8, dst: &mut [u8; TOTAL_SIZE], src: &[u8; TOTAL_SIZE]) {
        let (offset, size) = Self::get(bank);
        dst[offset..offset + size].copy_from_slice(&src[offset..offset + size]);
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

    pub const fn get(bank: u8) -> (usize, usize) {
        const BANKS: [(usize, usize); 9] = [
            (VramBanks::get_a(), BANK_A_SIZE),
            (VramBanks::get_b(), BANK_B_SIZE),
            (VramBanks::get_c(), BANK_C_SIZE),
            (VramBanks::get_d(), BANK_D_SIZE),
            (VramBanks::get_e(), BANK_E_SIZE),
            (VramBanks::get_f(), BANK_F_SIZE),
            (VramBanks::get_g(), BANK_G_SIZE),
            (VramBanks::get_h(), BANK_H_SIZE),
            (VramBanks::get_i(), BANK_I_SIZE),
        ];
        BANKS[bank as usize]
    }
}

pub const BG_A_SIZE: usize = 512 * 1024;
pub const OBJ_A_SIZE: usize = 256 * 1024;
pub const BG_B_SIZE: usize = 128 * 1024;
pub const OBJ_B_SIZE: usize = 128 * 1024;

pub const BG_EXT_PAL_SIZE: usize = 32 * 1024;
pub const OBJ_EXT_PAL_SIZE: usize = 8 * 1024;

pub const TEX_REAR_PLANE_IMAGE_SIZE: usize = 4 * 128 * 1024;
pub const TEX_PAL_SIZE: usize = 6 * 16 * 1024;

pub const LCDC_OFFSET: u32 = 0x800000;
pub const BG_A_OFFSET: u32 = 0x000000;
pub const OBJ_A_OFFSET: u32 = 0x400000;
pub const BG_B_OFFSET: u32 = 0x200000;
pub const OBJ_B_OFFSET: u32 = 0x600000;

pub const ARM7_SIZE: u32 = 128 * 1024;

#[derive(Clone, Default)]
pub struct VramMaps {
    lcdc: OverlapMapping<LCDC_OFFSET, TOTAL_SIZE, { 16 * 1024 }, 1>,

    bg_a: OverlapMapping<BG_B_OFFSET, { BG_A_SIZE }, { 16 * 1024 }, 7>,
    obj_a: OverlapMapping<OBJ_A_OFFSET, { 256 * 1024 }, { 16 * 1024 }, 5>,
    bg_ext_palette_a: [VramMap<{ BG_EXT_PAL_SIZE / 4 }>; 4],
    obj_ext_palette_a: VramMap<{ OBJ_EXT_PAL_SIZE }>,

    tex_rear_plane_img: [VramMap<{ 128 * 1024 }>; 4],
    tex_palette: [VramMap<{ 16 * 1024 }>; 6],

    bg_b: OverlapMapping<BG_B_OFFSET, { BG_B_SIZE }, { 16 * 1024 }, 3>,
    obj_b: OverlapMapping<OBJ_B_OFFSET, { 128 * 1024 }, { 16 * 1024 }, 2>,
    bg_ext_palette_b: [VramMap<{ BG_EXT_PAL_SIZE / 4 }>; 4],
    obj_ext_palette_b: VramMap<{ OBJ_EXT_PAL_SIZE }>,
}

impl VramMaps {
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

    pub fn read_all_lcdc(&self, buf: &mut [u8; TOTAL_SIZE], vram: &[u8; TOTAL_SIZE]) {
        self.lcdc.read_all(0, buf, vram)
    }

    pub fn read_all_bg_a(&self, buf: &mut [u8; BG_A_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        self.bg_a.read_all(0, buf, vram)
    }

    pub fn read_all_obj_a(&self, buf: &mut [u8; OBJ_A_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        self.obj_a.read_all(0, buf, vram)
    }

    pub fn read_all_bg_a_ext_palette(&self, buf: &mut [u8; BG_EXT_PAL_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        for i in 0..self.bg_ext_palette_a.len() {
            let map = &self.bg_ext_palette_a[i];
            let buf = &mut buf[i << 13..(i << 13) + 8 * 1024];
            if !map.is_null() {
                buf.copy_from_slice(map.as_ref(vram));
            } else {
                buf.fill(0);
            }
        }
    }

    pub fn read_all_obj_a_ext_palette(&self, buf: &mut [u8; OBJ_EXT_PAL_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        if !self.obj_ext_palette_a.is_null() {
            buf.copy_from_slice(self.obj_ext_palette_a.as_ref(vram));
        } else {
            buf.fill(0);
        }
    }

    pub fn read_bg_b(&self, buf: &mut [u8; BG_B_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        self.bg_b.read_all(0, buf, vram)
    }

    pub fn read_all_obj_b(&self, buf: &mut [u8; OBJ_B_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        self.obj_b.read_all(0, buf, vram)
    }

    pub fn read_all_bg_b_ext_palette(&self, buf: &mut [u8; BG_EXT_PAL_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        for i in 0..self.bg_ext_palette_b.len() {
            let map = &self.bg_ext_palette_b[i];
            let buf = &mut buf[i << 13..(i << 13) + 8 * 1024];
            if !map.is_null() {
                buf.copy_from_slice(map.as_ref(vram));
            } else {
                buf.fill(0);
            }
        }
    }

    pub fn read_all_obj_b_ext_palette(&self, buf: &mut [u8; OBJ_EXT_PAL_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        if !self.obj_ext_palette_b.is_null() {
            buf.copy_from_slice(self.obj_ext_palette_b.as_ref(vram));
        } else {
            buf.fill(0);
        }
    }

    pub fn read_all_tex_rear_plane_img(&self, buf: &mut [u8; TEX_REAR_PLANE_IMAGE_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        for i in 0..self.tex_rear_plane_img.len() {
            let map = &self.tex_rear_plane_img[i];
            let buf = &mut buf[i << 17..(i << 17) + 128 * 1024];
            if !map.is_null() {
                buf.copy_from_slice(map.as_ref(vram));
            } else {
                buf.fill(0);
            }
        }
    }

    pub fn read_all_tex_palette(&self, buf: &mut [u8; TEX_PAL_SIZE as usize], vram: &[u8; TOTAL_SIZE]) {
        for i in 0..self.tex_palette.len() {
            let map = &self.tex_palette[i];
            let buf = &mut buf[i << 14..(i << 14) + 16 * 1024];
            if !map.is_null() {
                buf.copy_from_slice(map.as_ref(vram));
            } else {
                buf.fill(0);
            }
        }
    }
}

#[derive(Default)]
pub struct Vram {
    pub stat: u8,
    pub cnt: [u8; BANK_SIZE],
    pub maps: VramMaps,
    arm7: OverlapMapping<0x000000, { 128 * 2 * 1024 }, { ARM7_SIZE as usize }, 2>,
    map_addrs: [(u32, usize); 9],
}

impl Vram {
    pub fn rebuild_maps(&mut self) {
        self.stat = 0;
        self.maps.reset();
        self.arm7.reset();
        self.map_addrs.fill((0, 0));

        {
            let cnt_a = VramCnt::from(self.cnt[0]);
            if cnt_a.enable() {
                let mst = u8::from(cnt_a.mst()) & 0x3;
                match mst {
                    0 => {
                        let map: VramMap<BANK_A_SIZE> = VramMap::new(VramBanks::get_a());
                        self.maps.lcdc.add::<BANK_A_SIZE>(map, 0, &mut self.map_addrs[0]);
                    }
                    1 => {
                        let ofs = u8::from(cnt_a.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_A_SIZE>(VramMap::new(VramBanks::get_a()), 128 / 16 * ofs, &mut self.map_addrs[0]);
                    }
                    2 => {
                        let ofs = u8::from(cnt_a.ofs()) as usize;
                        self.maps.obj_a.add::<BANK_A_SIZE>(VramMap::new(VramBanks::get_a()), 128 / 16 * (ofs & 1), &mut self.map_addrs[0]);
                    }
                    3 => {
                        let ofs = u8::from(cnt_a.ofs()) as usize;
                        self.maps.tex_rear_plane_img[ofs] = VramMap::new(VramBanks::get_a());
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_b = VramCnt::from(self.cnt[1]);
            if cnt_b.enable() {
                let mst = u8::from(cnt_b.mst()) & 0x3;
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_B_SIZE>(VramMap::new(VramBanks::get_b()), BANK_A_SIZE / 1024 / 16, &mut self.map_addrs[1]);
                    }
                    1 => {
                        let ofs = u8::from(cnt_b.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_B_SIZE>(VramMap::new(VramBanks::get_b()), 128 / 16 * ofs, &mut self.map_addrs[1]);
                    }
                    2 => {
                        let ofs = u8::from(cnt_b.ofs()) as usize;
                        self.maps.obj_a.add::<BANK_B_SIZE>(VramMap::new(VramBanks::get_b()), 128 / 16 * (ofs & 1), &mut self.map_addrs[1]);
                    }
                    3 => {
                        let ofs = u8::from(cnt_b.ofs()) as usize;
                        self.maps.tex_rear_plane_img[ofs] = VramMap::new(VramBanks::get_b());
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_c = VramCnt::from(self.cnt[2]);
            if cnt_c.enable() {
                let mst = u8::from(cnt_c.mst());
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_C_SIZE>(VramMap::new(VramBanks::get_c()), BANK_A_SIZE / 1024 / 16 * 2, &mut self.map_addrs[2]);
                    }
                    1 => {
                        let ofs = u8::from(cnt_c.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_C_SIZE>(VramMap::new(VramBanks::get_c()), 128 / 16 * ofs, &mut self.map_addrs[2]);
                    }
                    2 => {
                        let ofs = u8::from(cnt_c.ofs()) as usize;
                        self.arm7.add::<BANK_C_SIZE>(VramMap::new(VramBanks::get_c()), ofs & 1, &mut self.map_addrs[2]);
                        self.stat |= 1;
                    }
                    3 => {
                        let ofs = u8::from(cnt_c.ofs()) as usize;
                        self.maps.tex_rear_plane_img[ofs] = VramMap::new(VramBanks::get_c());
                    }
                    4 => {
                        self.maps.bg_b.add::<BANK_C_SIZE>(VramMap::new(VramBanks::get_c()), 0, &mut self.map_addrs[2]);
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_d = VramCnt::from(self.cnt[3]);
            if cnt_d.enable() {
                let mst = u8::from(cnt_d.mst());
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_D_SIZE>(VramMap::new(VramBanks::get_d()), BANK_A_SIZE / 1024 / 16 * 3, &mut self.map_addrs[3]);
                    }
                    1 => {
                        let ofs = u8::from(cnt_d.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_D_SIZE>(VramMap::new(VramBanks::get_d()), 128 / 16 * ofs, &mut self.map_addrs[3]);
                    }
                    2 => {
                        let ofs = u8::from(cnt_d.ofs()) as usize;
                        self.arm7.add::<BANK_D_SIZE>(VramMap::new(VramBanks::get_d()), ofs & 1, &mut self.map_addrs[3]);
                        self.stat |= 2;
                    }
                    3 => {
                        let ofs = u8::from(cnt_d.ofs()) as usize;
                        self.maps.tex_rear_plane_img[ofs] = VramMap::new(VramBanks::get_d());
                    }
                    4 => {
                        self.maps.obj_b.add::<BANK_D_SIZE>(VramMap::new(VramBanks::get_d()), 0, &mut self.map_addrs[3]);
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_e = VramCnt::from(self.cnt[4]);
            if cnt_e.enable() {
                let mst = u8::from(cnt_e.mst());
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_E_SIZE>(VramMap::new(VramBanks::get_e()), BANK_A_SIZE / 1024 / 16 * 4, &mut self.map_addrs[4]);
                    }
                    1 => {
                        self.maps.bg_a.add::<BANK_E_SIZE>(VramMap::new(VramBanks::get_e()), 0, &mut self.map_addrs[4]);
                    }
                    2 => {
                        self.maps.obj_a.add::<BANK_E_SIZE>(VramMap::new(VramBanks::get_e()), 0, &mut self.map_addrs[4]);
                    }
                    3 => {
                        let vram_map = VramMap::<BANK_E_SIZE>::new(VramBanks::get_e());
                        for i in 0..4 {
                            self.maps.tex_palette[i] = vram_map.extract_section(i);
                        }
                    }
                    4 => {
                        let vram_map = VramMap::<BANK_E_SIZE>::new(VramBanks::get_e());
                        for i in 0..4 {
                            self.maps.bg_ext_palette_a[i] = vram_map.extract_section(i);
                        }
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_f = VramCnt::from(self.cnt[5]);
            if cnt_f.enable() {
                let mst = u8::from(cnt_f.mst());
                match mst {
                    0 => {
                        self.maps
                            .lcdc
                            .add::<BANK_F_SIZE>(VramMap::new(VramBanks::get_f()), (BANK_A_SIZE * 4 + BANK_E_SIZE) / 1024 / 16, &mut self.map_addrs[5]);
                    }
                    1 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_F_SIZE>(VramMap::new(VramBanks::get_f()), (ofs & 1) + 2 * (ofs & 2), &mut self.map_addrs[5]);
                    }
                    2 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        self.maps.obj_a.add::<BANK_F_SIZE>(VramMap::new(VramBanks::get_f()), (ofs & 1) + 2 * (ofs & 2), &mut self.map_addrs[5]);
                    }
                    3 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        self.maps.tex_palette[(ofs & 1) + ((ofs & 2) * 2)] = VramMap::new(VramBanks::get_f());
                    }
                    4 => {
                        let ofs = u8::from(cnt_f.ofs()) as usize;
                        let vram_map = VramMap::<BANK_F_SIZE>::new(VramBanks::get_f());
                        for i in 0..2 {
                            self.maps.bg_ext_palette_a[(ofs & 1) * 2 + i] = vram_map.extract_section(i);
                        }
                    }
                    5 => {
                        self.maps.obj_ext_palette_a = VramMap::<BANK_F_SIZE>::new(VramBanks::get_f()).extract_section(0);
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_g = VramCnt::from(self.cnt[6]);
            if cnt_g.enable() {
                let mst = u8::from(cnt_g.mst());
                match mst {
                    0 => {
                        self.maps
                            .lcdc
                            .add::<BANK_G_SIZE>(VramMap::new(VramBanks::get_g()), (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE) / 1024 / 16, &mut self.map_addrs[6]);
                    }
                    1 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        self.maps.bg_a.add::<BANK_G_SIZE>(VramMap::new(VramBanks::get_g()), (ofs & 1) + 2 * (ofs & 2), &mut self.map_addrs[6]);
                    }
                    2 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        self.maps.obj_a.add::<BANK_G_SIZE>(VramMap::new(VramBanks::get_g()), (ofs & 1) + 2 * (ofs & 2), &mut self.map_addrs[6]);
                    }
                    3 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        self.maps.tex_palette[((ofs & 2) << 1) + (ofs & 1)] = VramMap::new(VramBanks::get_g())
                    }
                    4 => {
                        let ofs = u8::from(cnt_g.ofs()) as usize;
                        let vram_map = VramMap::<BANK_G_SIZE>::new(VramBanks::get_g());
                        for i in 0..2 {
                            self.maps.bg_ext_palette_a[(ofs & 1) * 2 + i] = vram_map.extract_section(i);
                        }
                    }
                    5 => {
                        self.maps.obj_ext_palette_a = VramMap::<BANK_G_SIZE>::new(VramBanks::get_g()).extract_section(0);
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_h = VramCnt::from(self.cnt[7]);
            if cnt_h.enable() {
                let mst = u8::from(cnt_h.mst()) & 0x3;
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_H_SIZE>(
                            VramMap::new(VramBanks::get_h()),
                            (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE) / 1024 / 16,
                            &mut self.map_addrs[7],
                        );
                    }
                    1 => {
                        self.maps.bg_b.add::<BANK_H_SIZE>(VramMap::new(VramBanks::get_h()), 0, &mut self.map_addrs[7]);
                    }
                    2 => {
                        let vram_map = VramMap::<BANK_H_SIZE>::new(VramBanks::get_h());
                        for i in 0..4 {
                            self.maps.bg_ext_palette_b[i] = vram_map.extract_section(i);
                        }
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }

        {
            let cnt_i = VramCnt::from(self.cnt[8]);
            if cnt_i.enable() {
                let mst = u8::from(cnt_i.mst()) & 0x3;
                match mst {
                    0 => {
                        self.maps.lcdc.add::<BANK_I_SIZE>(
                            VramMap::new(VramBanks::get_i()),
                            (BANK_A_SIZE * 4 + BANK_E_SIZE + BANK_F_SIZE + BANK_G_SIZE + BANK_H_SIZE) / 1024 / 16,
                            &mut self.map_addrs[8],
                        );
                    }
                    1 => {
                        self.maps.bg_b.add::<BANK_I_SIZE>(VramMap::new(VramBanks::get_i()), 2, &mut self.map_addrs[8]);
                    }
                    2 => {
                        self.maps.obj_b.add(VramMap::<BANK_I_SIZE>::new(VramBanks::get_i()), 0, &mut self.map_addrs[8]);
                    }
                    3 => {
                        self.maps.obj_ext_palette_b = VramMap::<BANK_I_SIZE>::new(VramBanks::get_i()).extract_section(0);
                    }
                    _ => unsafe { unreachable_unchecked() },
                }
            }
        }
    }

    pub fn get_shm_offset<const CPU: CpuType>(&self, addr: u32) -> usize {
        let base_addr = addr & 0xF00000;
        let addr_offset = addr & 0xFFFFF;
        let offset = match CPU {
            ARM9 => match base_addr {
                BG_A_OFFSET => self.maps.bg_a.get_offset(addr_offset),
                OBJ_A_OFFSET => self.maps.obj_a.get_offset(addr_offset),
                BG_B_OFFSET => self.maps.bg_b.get_offset(addr_offset),
                OBJ_B_OFFSET => self.maps.obj_b.get_offset(addr_offset),
                _ => self.maps.lcdc.get_offset(addr_offset),
            },
            ARM7 => self.arm7.get_offset(addr_offset),
        };
        if offset == 0 {
            0
        } else {
            regions::VRAM_REGION.shm_offset + offset
        }
    }
}

impl Emu {
    pub fn vram_get_mem(&self) -> &'static [u8; TOTAL_SIZE] {
        unsafe { mem::transmute(self.mem.shm.as_ptr().add(regions::VRAM_REGION.shm_offset)) }
    }

    pub fn vram_get_mem_mut(&mut self) -> &'static mut [u8; TOTAL_SIZE] {
        unsafe { mem::transmute(self.mem.shm.as_ptr().add(regions::VRAM_REGION.shm_offset)) }
    }

    pub fn vram_read<const CPU: CpuType, T: utils::Convert>(&self, addr: u32) -> T {
        let base_addr = addr & 0xF00000;
        let addr_offset = addr & 0xFFFFF;
        let vram = self.vram_get_mem();
        match CPU {
            ARM9 => match base_addr {
                BG_A_OFFSET => self.mem.vram.maps.bg_a.read(addr_offset, vram),
                OBJ_A_OFFSET => self.mem.vram.maps.obj_a.read(addr_offset, vram),
                BG_B_OFFSET => self.mem.vram.maps.bg_b.read(addr_offset, vram),
                OBJ_B_OFFSET => self.mem.vram.maps.obj_b.read(addr_offset, vram),
                _ => self.mem.vram.maps.lcdc.read(addr_offset, vram),
            },
            ARM7 => self.mem.vram.arm7.read(addr_offset, vram),
        }
    }

    pub fn vram_write<const CPU: CpuType, T: utils::Convert>(&mut self, addr: u32, value: T) {
        let base_addr = addr & 0xF00000;
        let addr_offset = addr & 0xFFFFF;
        let vram = self.vram_get_mem_mut();
        match CPU {
            ARM9 => match base_addr {
                BG_A_OFFSET => self.mem.vram.maps.bg_a.write(addr_offset, value, vram),
                OBJ_A_OFFSET => self.mem.vram.maps.obj_a.write(addr_offset, value, vram),
                BG_B_OFFSET => self.mem.vram.maps.bg_b.write(addr_offset, value, vram),
                OBJ_B_OFFSET => self.mem.vram.maps.obj_b.write(addr_offset, value, vram),
                _ => self.mem.vram.maps.lcdc.write(addr_offset, value, vram),
            },
            ARM7 => self.mem.vram.arm7.write(addr_offset, value, vram),
        };
    }

    pub fn vram_write_slice<const CPU: CpuType, T: utils::Convert>(&mut self, addr: u32, slice: &[T]) {
        let base_addr = addr & 0xF00000;
        let addr_offset = addr & 0xFFFFF;
        let vram = self.vram_get_mem_mut();
        match CPU {
            ARM9 => match base_addr {
                BG_A_OFFSET => self.mem.vram.maps.bg_a.write_slice(addr_offset, slice, vram),
                OBJ_A_OFFSET => self.mem.vram.maps.obj_a.write_slice(addr_offset, slice, vram),
                BG_B_OFFSET => self.mem.vram.maps.bg_b.write_slice(addr_offset, slice, vram),
                OBJ_B_OFFSET => self.mem.vram.maps.obj_b.write_slice(addr_offset, slice, vram),
                _ => self.mem.vram.maps.lcdc.write_slice(addr_offset, slice, vram),
            },
            ARM7 => self.mem.vram.arm7.write_slice(addr_offset, slice, vram),
        };
    }

    pub fn vram_set_cnt(&mut self, bank: usize, value: u8) {
        const MASKS: [u8; 9] = [0x9B, 0x9B, 0x9F, 0x9F, 0x87, 0x9F, 0x9F, 0x83, 0x83];
        unsafe { assert_unchecked(bank < MASKS.len()) };
        let value = value & MASKS[bank];
        if self.mem.vram.cnt[bank] == value {
            return;
        }
        self.mem.vram.cnt[bank] = value;

        let (current_mapped_addr, current_mapped_size) = self.mem.vram.map_addrs[bank];
        if current_mapped_size != 0 {
            let start_addr = current_mapped_addr;
            let end_addr = current_mapped_addr + current_mapped_size as u32;
            if unlikely(self.mem.vram.stat != 0 && (bank == 2 || bank == 3)) {
                self.mmu_remove_vram::<{ ARM7 }>(start_addr, end_addr);
            } else {
                self.mmu_remove_vram::<{ ARM9 }>(start_addr, end_addr);
            }
        }

        debug_println!("Set vram cnt {bank:x} to {value:x}");

        self.mem.vram.rebuild_maps();

        self.jit.invalidate_vram();

        let (new_mapped_addr, new_mapped_size) = self.mem.vram.map_addrs[bank];
        if new_mapped_size != 0 {
            let start_addr = new_mapped_addr;
            let end_addr = new_mapped_addr + new_mapped_size as u32;
            if unlikely(self.mem.vram.stat != 0 && (bank == 2 || bank == 3)) {
                self.mmu_update_vram::<{ ARM7 }>(start_addr, end_addr);
            } else {
                self.mmu_update_vram::<{ ARM9 }>(start_addr, end_addr);
            }
        }
    }
}

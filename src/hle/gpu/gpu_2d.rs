use crate::hle::gpu::gpu::{DISPLAY_HEIGHT, DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH};
use crate::hle::memory::mem::Memory;
use crate::hle::memory::regions::VRAM_OFFSET;
use crate::hle::memory::vram;
use crate::hle::memory::vram::{BG_A_OFFSET, BG_B_OFFSET, OBJ_A_OFFSET, OBJ_B_OFFSET};
use crate::hle::CpuType::ARM9;
use crate::logging::debug_println;
use crate::utils;
use crate::utils::{HeapMemI8, HeapMemU32};
use bilge::prelude::*;
use std::hint::unreachable_unchecked;
use std::marker::ConstParamTy;
use std::mem;

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
struct DispCnt {
    bg_mode: u3,
    bg0_3d: u1,
    tile_obj_mapping: u1,
    bitmap_obj_2d: u1,
    bitmap_obj_mapping: u1,
    forced_blank: u1,
    screen_display_bg0: u1,
    screen_display_bg1: u1,
    screen_display_bg2: u1,
    screen_display_bg3: u1,
    screen_display_obj: u1,
    window0_display_flag: u1,
    window1_display_flag: u1,
    obj_window_display_flag: u1,
    display_mode: u2,
    vram_block: u2,
    tile_obj_1d_boundary: u2,
    bitmap_obj_1d_boundary: u1,
    obj_processing_during_h_blank: u1,
    char_base: u3,
    screen_base: u3,
    bg_extended_palettes: u1,
    obj_extended_palettes: u1,
}

impl Default for DispCnt {
    fn default() -> Self {
        DispCnt::from(0)
    }
}

impl DispCnt {
    fn is_any_window_enabled(&self) -> bool {
        bool::from(self.window0_display_flag())
            || bool::from(self.window1_display_flag())
            || bool::from(self.obj_window_display_flag())
    }
}

#[repr(u8)]
enum DisplayMode {
    Off = 0,
    Layers = 1,
    Vram = 2,
    MainMemory = 3,
}

impl From<u8> for DisplayMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= DisplayMode::MainMemory as u8);
        unsafe { mem::transmute(value) }
    }
}

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
struct BgCnt {
    priority: u2,
    char_base_block: u2,
    not_used: u2,
    mosaic: u1,
    color_palettes: u1,
    screen_base_block: u5,
    ext_palette_slot_display_area_overflow: u1,
    screen_size: u2,
}

impl Default for BgCnt {
    fn default() -> Self {
        BgCnt::from(0)
    }
}

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
struct BldCnt {
    bg0_1st_target_pixel: u1,
    bg1_1st_target_pixel: u1,
    bg2_1st_target_pixel: u1,
    bg3_1st_target_pixel: u1,
    obj_1st_target_pixel: u1,
    bd_1st_target_pixel: u1,
    color_special_effect: u2,
    bg0_2nc_target_pixel: u1,
    bg1_2nc_target_pixel: u1,
    bg2_2nc_target_pixel: u1,
    bg3_2nc_target_pixel: u1,
    obj_2nc_target_pixel: u1,
    bd_2nc_target_pixel: u1,
    not_used: u2,
}

impl Default for BldCnt {
    fn default() -> Self {
        BldCnt::from(0)
    }
}

#[bitsize(16)]
#[derive(FromBits)]
struct TextBgScreen {
    tile_num: u10,
    h_flip: u1,
    v_flip: u1,
    palette_num: u4,
}

#[derive(ConstParamTy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Gpu2DEngine {
    A,
    B,
}

#[derive(Default)]
struct Gpu2DInternal {
    x: [i32; 2],
    y: [i32; 2],
}

#[derive(Default)]
struct Gpu2DInner {
    disp_cnt: DispCnt,
    bg_cnt: [BgCnt; 4],
    bg_h_ofs: [u16; 4],
    bg_v_ofs: [u16; 4],
    bg_pa: [i16; 2],
    bg_pb: [i16; 2],
    bg_pc: [i16; 2],
    bg_pd: [i16; 2],
    bg_x: [i32; 2],
    bg_y: [i32; 2],
    bld_cnt: BldCnt,
    win_x1: [u8; 2],
    win_x2: [u8; 2],
    win_in: u16,
    win_out: u16,
    mosaic: u16,
    disp_stat: u16,
    pow_cnt1: u16,
    internal: Gpu2DInternal,
}

#[derive(ConstParamTy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Gpu2DLayer {
    A,
    B,
}

struct Gpu2DLayers {
    pixels: HeapMemU32<{ DISPLAY_WIDTH * 2 }>,
    priorities: HeapMemI8<{ DISPLAY_WIDTH * 2 }>,
    blend_bits: HeapMemI8<{ DISPLAY_WIDTH * 2 }>,
}

impl Gpu2DLayers {
    fn new() -> Self {
        let pixels = HeapMemU32::new();
        Gpu2DLayers {
            pixels,
            priorities: HeapMemI8::new(),
            blend_bits: HeapMemI8::new(),
        }
    }

    fn get_pixels_mut<const LAYER: Gpu2DLayer>(&mut self) -> &'static mut [u32; DISPLAY_WIDTH] {
        let ptr = match LAYER {
            Gpu2DLayer::A => self.pixels[..DISPLAY_WIDTH].as_mut_ptr(),
            Gpu2DLayer::B => self.pixels[DISPLAY_WIDTH..].as_mut_ptr(),
        };
        unsafe {
            (ptr as *mut [u32; DISPLAY_WIDTH])
                .as_mut()
                .unwrap_unchecked()
        }
    }

    fn get_priorities_mut<const LAYER: Gpu2DLayer>(&mut self) -> &'static mut [i8; DISPLAY_WIDTH] {
        let ptr = match LAYER {
            Gpu2DLayer::A => self.priorities[..DISPLAY_WIDTH].as_mut_ptr(),
            Gpu2DLayer::B => self.priorities[DISPLAY_WIDTH..].as_mut_ptr(),
        };
        unsafe {
            (ptr as *mut [i8; DISPLAY_WIDTH])
                .as_mut()
                .unwrap_unchecked()
        }
    }

    fn get_blend_bits_mut<const LAYER: Gpu2DLayer>(&mut self) -> &'static mut [i8; DISPLAY_WIDTH] {
        let ptr = match LAYER {
            Gpu2DLayer::A => self.blend_bits[..DISPLAY_WIDTH].as_mut_ptr(),
            Gpu2DLayer::B => self.blend_bits[DISPLAY_WIDTH..].as_mut_ptr(),
        };
        unsafe {
            (ptr as *mut [i8; DISPLAY_WIDTH])
                .as_mut()
                .unwrap_unchecked()
        }
    }
}

pub struct Gpu2D<const ENGINE: Gpu2DEngine> {
    inner: Gpu2DInner,
    layers: Gpu2DLayers,
    pub framebuffer: HeapMemU32<{ DISPLAY_PIXEL_COUNT }>,
}

impl<const ENGINE: Gpu2DEngine> Gpu2D<ENGINE> {
    const fn get_bg_offset() -> u32 {
        match ENGINE {
            Gpu2DEngine::A => BG_A_OFFSET,
            Gpu2DEngine::B => BG_B_OFFSET,
        }
    }

    const fn get_palettes_offset() -> u32 {
        match ENGINE {
            Gpu2DEngine::A => 0,
            Gpu2DEngine::B => 0x400,
        }
    }

    const fn get_obj_offset() -> u32 {
        match ENGINE {
            Gpu2DEngine::A => OBJ_A_OFFSET,
            Gpu2DEngine::B => OBJ_B_OFFSET,
        }
    }

    const fn get_oam_offset() -> u32 {
        match ENGINE {
            Gpu2DEngine::A => 0,
            Gpu2DEngine::B => 0x400,
        }
    }

    fn read_bg<T: utils::Convert>(&self, addr: u32, mem: &Memory) -> T {
        mem.vram.read::<{ ARM9 }, _>(Self::get_bg_offset() + addr)
    }

    fn read_obj<T: utils::Convert>(&self, addr: u32, mem: &Memory) -> T {
        mem.vram.read::<{ ARM9 }, _>(Self::get_obj_offset() + addr)
    }

    fn read_palettes<T: utils::Convert>(&self, addr: u32, mem: &Memory) -> T {
        mem.palettes.read(Self::get_palettes_offset() + addr)
    }

    fn read_oam<T: utils::Convert>(&self, addr: u32, mem: &Memory) -> T {
        mem.oam.read(Self::get_oam_offset() + addr)
    }

    pub fn new() -> Self {
        Gpu2D {
            inner: Gpu2DInner::default(),
            layers: Gpu2DLayers::new(),
            framebuffer: HeapMemU32::new(),
        }
    }

    pub fn get_disp_cnt(&self) -> u32 {
        self.inner.disp_cnt.into()
    }

    pub fn get_bg_cnt(&self, bg_num: usize) -> u16 {
        self.inner.bg_cnt[bg_num].into()
    }

    pub fn get_win_in(&self) -> u16 {
        self.inner.win_in
    }

    pub fn get_win_out(&self) -> u16 {
        self.inner.win_out
    }

    pub fn set_disp_cnt(&mut self, mut mask: u32, value: u32) {
        if ENGINE == Gpu2DEngine::B {
            mask &= 0xC0B1FFF7;
        }
        self.inner.disp_cnt = ((u32::from(self.inner.disp_cnt) & !mask) | (value & mask)).into();
        debug_println!(
            "GPU engine {:?} set disp cnt {:x} {}",
            ENGINE,
            u32::from(self.inner.disp_cnt),
            (u32::from(self.inner.disp_cnt) >> 16) & 0x3
        );
    }

    pub fn set_bg_cnt(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.inner.bg_cnt[bg_num] =
            ((u16::from(self.inner.bg_cnt[bg_num]) & !mask) | (value & mask)).into();
    }

    pub fn set_bg_h_ofs(&mut self, bg_num: usize, mut mask: u16, value: u16) {
        mask &= 0x01FF;
        self.inner.bg_h_ofs[bg_num] = (self.inner.bg_h_ofs[bg_num] & !mask) | (value & mask);
    }

    pub fn set_bg_v_ofs(&mut self, bg_num: usize, mut mask: u16, value: u16) {
        mask &= 0x01FF;
        self.inner.bg_v_ofs[bg_num] = (self.inner.bg_v_ofs[bg_num] & !mask) | (value & mask);
    }

    pub fn set_bg_pa(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.inner.bg_pa[bg_num - 2] =
            ((self.inner.bg_pa[bg_num - 2] as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_bg_pb(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.inner.bg_pb[bg_num - 2] =
            ((self.inner.bg_pb[bg_num - 2] as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_bg_pc(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.inner.bg_pc[bg_num - 2] =
            ((self.inner.bg_pc[bg_num - 2] as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_bg_pd(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.inner.bg_pd[bg_num - 2] =
            ((self.inner.bg_pd[bg_num - 2] as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_bg_x(&mut self, bg_num: usize, mut mask: u32, value: u32) {
        mask &= 0x0FFFFFFF;
        let mut bg_x = (self.inner.bg_x[bg_num - 2] as u32 & !mask) | (value & mask);
        if bg_x & (1 << 27) != 0 {
            bg_x |= 0xF0000000;
        } else {
            bg_x &= !0xF0000000;
        }
        let bg_x = bg_x as i32;
        self.inner.internal.x[bg_num - 2] = bg_x;
        self.inner.bg_x[bg_num - 2] = bg_x;
    }

    pub fn set_bg_y(&mut self, bg_num: usize, mut mask: u32, value: u32) {
        mask &= 0x0FFFFFFF;
        let mut bg_y = (self.inner.bg_y[bg_num - 2] as u32 & !mask) | (value & mask);
        if bg_y & (1 << 27) != 0 {
            bg_y |= 0xF0000000;
        } else {
            bg_y &= !0xF0000000;
        }
        let bg_y = bg_y as i32;
        self.inner.internal.y[bg_num - 2] = bg_y;
        self.inner.bg_y[bg_num - 2] = bg_y;
    }

    pub fn set_win_h(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_win_v(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_win_in(&mut self, mut mask: u16, value: u16) {
        mask &= 0x3F3F;
        self.inner.win_in = (self.inner.win_in & !mask) | (value & mask);
    }

    pub fn set_win_out(&mut self, mut mask: u16, value: u16) {
        mask &= 0x3F3F;
        self.inner.win_out = (self.inner.win_out & !mask) | (value & mask);
    }

    pub fn set_mosaic(&mut self, mask: u16, value: u16) {
        self.inner.mosaic = (self.inner.mosaic & !mask) | (value & mask);
    }

    pub fn set_bld_cnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0x3FFF;
        self.inner.bld_cnt = ((u16::from(self.inner.bld_cnt) & !mask) | (value & mask)).into();
    }

    pub fn set_bld_alpha(&mut self, mask: u16, value: u16) {}

    pub fn set_bld_y(&mut self, value: u8) {}

    pub fn set_master_bright(&mut self, mask: u16, value: u16) {}

    pub fn draw_scanline(&mut self, line: u8, mem: &Memory) {
        let backdrop = self.read_palettes::<u16>(0, mem);
        let backdrop = backdrop & !(1 << 15);
        self.layers.pixels.fill(backdrop as u32);
        self.layers.priorities.fill(4);
        self.layers.blend_bits.fill(5);

        let disp_cnt = self.inner.disp_cnt;

        if bool::from(disp_cnt.screen_display_obj()) {
            if bool::from(disp_cnt.obj_window_display_flag()) {
                self.draw_objects::<true>(line, mem);
            }
            self.draw_objects::<false>(line, mem);
        }

        macro_rules! draw {
            ($bg3:expr, $bg2:expr, $bg1:expr, $bg0:expr) => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    $bg3(self, line, mem);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    $bg2(self, line, mem);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    $bg1(self, line, mem);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    $bg0(self, line, mem);
                }
            };
        }

        match u8::from(disp_cnt.bg_mode()) {
            0 => {
                draw!(
                    Self::draw_text::<3>,
                    Self::draw_text::<2>,
                    Self::draw_text::<1>,
                    Self::draw_text::<0>
                );
            }
            1 => {
                draw!(
                    Self::draw_affine::<3>,
                    Self::draw_text::<2>,
                    Self::draw_text::<1>,
                    Self::draw_text::<0>
                );
            }
            2 => {
                draw!(
                    Self::draw_affine::<3>,
                    Self::draw_affine::<2>,
                    Self::draw_text::<1>,
                    Self::draw_text::<0>
                );
            }
            3 => {
                draw!(
                    Self::draw_extended::<3>,
                    Self::draw_text::<2>,
                    Self::draw_text::<1>,
                    Self::draw_text::<0>
                );
            }
            4 => {
                draw!(
                    Self::draw_extended::<3>,
                    Self::draw_affine::<2>,
                    Self::draw_text::<1>,
                    Self::draw_text::<0>
                );
            }
            5 => {
                draw!(
                    Self::draw_extended::<3>,
                    Self::draw_extended::<2>,
                    Self::draw_text::<1>,
                    Self::draw_text::<0>
                );
            }
            6 => {
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_large::<2>(line, mem);
                }
            }
            7 => {
                debug_println!("Unknown engine {:?} bg mode {}", ENGINE, disp_cnt.bg_mode());
            }
            _ => {
                unsafe { unreachable_unchecked() };
            }
        }

        let pixels_a = self.layers.get_pixels_mut::<{ Gpu2DLayer::A }>();
        let pixels_b = self.layers.get_pixels_mut::<{ Gpu2DLayer::B }>();
        let blend_bits_a = self.layers.get_blend_bits_mut::<{ Gpu2DLayer::A }>();
        let blend_bits_b = self.layers.get_blend_bits_mut::<{ Gpu2DLayer::B }>();

        let bld_mode = u8::from(self.inner.bld_cnt.color_special_effect());
        let bld_cnt_raw = u16::from(self.inner.bld_cnt);

        for i in 0..DISPLAY_WIDTH {
            let value = pixels_a[i];
            if value & (1 << 26) != 0 {
                // TODO 3d
            } else {
                pixels_a[i] = Self::rgb5_to_rgb6(value);
                if pixels_a[i] & (1 << 25) != 0 {
                    if bld_cnt_raw & (1 << (8 + blend_bits_b[i])) != 0 {
                        // TODO
                    } else if bld_mode < 2 || (bld_cnt_raw & (1 << blend_bits_a[i])) == 0 {
                        continue;
                    }
                } else if bld_mode == 0
                    || (bld_cnt_raw & (1 << blend_bits_a[i])) == 0
                    || (bld_mode == 1 && (bld_cnt_raw & (1 << (8 + blend_bits_b[i]))) == 0)
                {
                    continue;
                }
            }
        }

        let fb_start = line as usize * DISPLAY_WIDTH;
        let fb_end = fb_start + DISPLAY_WIDTH;
        let fb = &mut self.framebuffer[fb_start..fb_end];
        match DisplayMode::from(u8::from(disp_cnt.display_mode())) {
            DisplayMode::Off => fb.fill(!0),
            DisplayMode::Layers => fb.copy_from_slice(pixels_a),
            DisplayMode::Vram => {
                let vram_block = u32::from(disp_cnt.vram_block());
                let base_addr = vram::LCDC_OFFSET
                    + vram_block * vram::BANK_A_SIZE as u32
                    + ((fb_start as u32) << 1);

                fb.iter_mut().enumerate().for_each(|(i, value)| {
                    *value = Self::rgb5_to_rgb6(
                        mem.vram
                            .read::<{ ARM9 }, u16>(base_addr + ((i as u32) << 1))
                            as u32,
                    );
                });
            }
            DisplayMode::MainMemory => {
                todo!()
            }
        }
    }

    fn draw_affine<const BG: usize>(&mut self, line: u8, mem: &Memory) {
        todo!()
    }

    fn draw_text<const BG: usize>(&mut self, line: u8, mem: &Memory) {
        if BG == 0 && bool::from(self.inner.disp_cnt.bg0_3d()) {
            // TODO 3d
            return;
        }

        if bool::from(self.inner.bg_cnt[BG].color_palettes()) {
            self.draw_text_pixels::<BG, true>(line, mem);
        } else {
            self.draw_text_pixels::<BG, false>(line, mem);
        }
    }

    fn draw_text_pixels<const BG: usize, const BIT8: bool>(&mut self, line: u8, mem: &Memory) {
        let disp_cnt = self.inner.disp_cnt;
        let bg_cnt = self.inner.bg_cnt[BG];

        let mut tile_base_addr = (u32::from(disp_cnt.screen_base()) << 16)
            + (u32::from(bg_cnt.screen_base_block()) << 11);
        let index_base_addr =
            (u32::from(disp_cnt.char_base()) << 16) + (u32::from(bg_cnt.char_base_block()) << 14);

        let y_offset = (if bool::from(bg_cnt.mosaic()) && self.inner.mosaic != 0 {
            todo!()
        } else {
            line as u16
        } + self.inner.bg_v_ofs[BG])
            & 0x1FF;

        tile_base_addr += (y_offset as u32 & 0xF8) << 3;
        if y_offset >= 256 && (u8::from(bg_cnt.screen_size()) & 1) != 0 {
            tile_base_addr += if u8::from(bg_cnt.screen_size()) != 0 {
                0x1000
            } else {
                0x800
            };
        }

        let mut palettes_base_addr = Self::get_palettes_offset();
        let read_palettes = if BIT8 && bool::from(disp_cnt.bg_extended_palettes()) {
            palettes_base_addr = 0;
            if BG < 2 && bool::from(bg_cnt.ext_palette_slot_display_area_overflow()) {
                if !mem.vram.is_bg_ext_palette_mapped::<ENGINE>(BG + 2) {
                    return;
                }
                |mem: &Memory, addr: u32| mem.vram.read_bg_ext_palette::<ENGINE, u16>(BG + 2, addr)
            } else {
                if !mem.vram.is_bg_ext_palette_mapped::<ENGINE>(BG) {
                    return;
                }
                |mem: &Memory, addr: u32| mem.vram.read_bg_ext_palette::<ENGINE, u16>(BG, addr)
            }
        } else {
            |mem: &Memory, addr: u32| mem.palettes.read(addr)
        };

        for i in (0..DISPLAY_WIDTH as u32).step_by(8) {
            let x_offset = (i + self.inner.bg_h_ofs[BG] as u32) & 0x1FF;
            let tile_addr = tile_base_addr + ((x_offset & 0xF8) >> 2);

            if x_offset >= 256 && (u8::from(bg_cnt.screen_size()) & 2) != 0 {
                todo!()
            }

            let tile = self.read_bg::<u16>(tile_addr, mem);
            let tile = TextBgScreen::from(tile);

            let palette_addr = palettes_base_addr
                + if BIT8 {
                    if bool::from(disp_cnt.bg_extended_palettes()) {
                        u32::from(tile.palette_num()) << 9
                    } else {
                        0
                    }
                } else {
                    u32::from(tile.palette_num()) << 5
                };

            let index_addr = index_base_addr
                + if BIT8 {
                    (u32::from(tile.tile_num()) << 6)
                        + (if bool::from(tile.v_flip()) {
                            7 - (y_offset as u32 & 7)
                        } else {
                            y_offset as u32 & 7
                        } << 3)
                } else {
                    (u32::from(tile.tile_num()) << 5)
                        + (if bool::from(tile.v_flip()) {
                            7 - (y_offset as u32 & 7)
                        } else {
                            y_offset as u32 & 7
                        } << 2)
                };
            let mut indices = if BIT8 {
                self.read_bg::<u32>(index_addr, mem) as u64
                    | ((self.read_bg::<u32>(index_addr + 4, mem) as u64) << 32)
            } else {
                self.read_bg::<u32>(index_addr, mem) as u64
            };

            let mut x = i.wrapping_sub(x_offset & 7);
            while indices != 0 {
                let tmp_x = if bool::from(tile.h_flip()) {
                    7u32.wrapping_sub(x)
                } else {
                    x
                };
                if tmp_x < 256 && (indices & if BIT8 { 0xFF } else { 0xF }) != 0 {
                    let color = read_palettes(
                        mem,
                        palette_addr + ((indices as u32 & if BIT8 { 0xFF } else { 0xF }) << 1),
                    );
                    self.draw_bg_pixel::<BG>(line, tmp_x as usize, (color | (1 << 15)) as u32);
                }
                x = x.wrapping_add(1);
                if BIT8 {
                    indices >>= 8;
                } else {
                    indices >>= 4;
                }
            }
        }
    }

    fn draw_extended<const BG: usize>(&mut self, line: u8, mem: &Memory) {
        let mut rot_scale_x = self.inner.internal.x[BG - 2] - self.inner.bg_pa[BG - 2] as i32;
        let mut rot_scale_y = self.inner.internal.y[BG - 2] - self.inner.bg_pc[BG - 2] as i32;

        let bg_cnt = self.inner.bg_cnt[BG];

        if bool::from(bg_cnt.color_palettes()) {
            let base_data_addr = VRAM_OFFSET + (u32::from(bg_cnt.screen_base_block()) << 11);

            let (size_x, size_y) = match u8::from(bg_cnt.screen_size()) {
                0 => (128, 128),
                1 => (256, 256),
                2 => (512, 256),
                3 => (512, 512),
                _ => unsafe { unreachable_unchecked() },
            };

            if u8::from(bg_cnt.char_base_block()) & 1 != 0 {
                for i in 0..DISPLAY_WIDTH {
                    rot_scale_x += self.inner.bg_pa[BG - 2] as i32;
                    rot_scale_y += self.inner.bg_pc[BG - 2] as i32;
                    let mut x = rot_scale_x >> 8;
                    let mut y = rot_scale_y >> 8;

                    if bool::from(bg_cnt.ext_palette_slot_display_area_overflow()) {
                        x &= size_x - 1;
                        y &= size_y - 1;
                    } else if x < 0 || x >= size_x || y < 0 || y >= size_y {
                        continue;
                    }

                    let pixel = mem.vram.read::<{ ARM9 }, u16>(
                        (base_data_addr as i32 + (y * size_x + x) * 2) as u32,
                    );
                    if pixel & (1 << 15) != 0 {
                        self.draw_bg_pixel::<BG>(line, i, pixel as u32);
                    }
                }
            } else {
                for i in 0..DISPLAY_WIDTH {
                    rot_scale_x += self.inner.bg_pa[BG - 2] as i32;
                    rot_scale_y += self.inner.bg_pc[BG - 2] as i32;
                    let mut x = rot_scale_x >> 8;
                    let mut y = rot_scale_y >> 8;

                    if bool::from(bg_cnt.ext_palette_slot_display_area_overflow()) {
                        x &= size_x - 1;
                        y &= size_y - 1;
                    } else if x < 0 || x >= size_x || y < 0 || y >= size_y {
                        continue;
                    }

                    let index = mem
                        .vram
                        .read::<{ ARM9 }, u8>((base_data_addr as i32 + y * size_x + x) as u32);
                    if index != 0 {
                        let pixel = mem.palettes.read::<u16>(index as u32 * 2) | (1 << 15);
                        self.draw_bg_pixel::<BG>(line, i, pixel as u32);
                    }
                }
            }
        } else {
            todo!()
        }

        self.inner.internal.x[BG - 2] += self.inner.bg_pb[BG - 2] as i32;
        self.inner.internal.y[BG - 2] += self.inner.bg_pd[BG - 2] as i32;
    }

    fn draw_large<const BG: usize>(&self, line: u8, mem: &Memory) {
        todo!()
    }

    fn draw_objects<const WINDOW: bool>(&mut self, line: u8, mem: &Memory) {
        let bound = if bool::from(self.inner.disp_cnt.tile_obj_mapping()) {
            32u32 << u8::from(self.inner.disp_cnt.tile_obj_1d_boundary())
        } else {
            32u32
        };

        let read_palette = if bool::from(self.inner.disp_cnt.obj_extended_palettes()) {
            if !mem.vram.is_obj_ext_palette_mapped::<ENGINE>() {
                return;
            }
            |mem: &Memory, addr: u32| mem.vram.read_obj_ext_palette::<ENGINE, u16>(addr)
        } else {
            |mem: &Memory, addr: u32| mem.palettes.read::<u16>(addr)
        };

        for i in 0..128 {
            let byte = self.read_oam::<u8>(i * 8 + 1, mem);
            let type_ = (byte >> 2) & 0x3;

            if (byte & 0x3) == 2 || (type_ == 2) != WINDOW {
                continue;
            }

            let object = [
                self.read_oam::<u16>(i * 8, mem),
                self.read_oam::<u16>(i * 8 + 2, mem),
                self.read_oam::<u16>(i * 8 + 4, mem),
            ];

            let (width, height) = match ((object[0] >> 12) & 0xC) | ((object[1] >> 14) & 0x3) {
                0x0 => (8u32, 8u32),
                0x1 => (16, 16),
                0x2 => (32, 32),
                0x3 => (64, 64),
                0x4 => (16, 8),
                0x5 => (32, 8),
                0x6 => (32, 16),
                0x7 => (64, 32),
                0x8 => (8, 16),
                0x9 => (8, 32),
                0xA => (16, 32),
                0xB => (32, 64),
                _ => {
                    continue;
                }
            };

            let (width2, height2) = if (object[0] & 0x300) == 0x300 {
                (width << 1, height << 1)
            } else {
                (width, height)
            };

            let mut y = (object[0] & 0xFF) as i32;
            if y >= DISPLAY_HEIGHT as i32 {
                y -= DISPLAY_WIDTH as i32;
            }

            let sprite_y = if (object[0] & (1 << 12)) != 0 {
                line as i32 - (line as u16 % (((self.inner.mosaic >> 12) & 0xF) + 1)) as i32
            } else {
                line as i32
            } - y;
            if sprite_y < 0 || sprite_y >= height2 as i32 {
                continue;
            }

            let mut x = (object[1] & 0x1FF) as i32;
            if x >= DISPLAY_WIDTH as i32 {
                x -= DISPLAY_WIDTH as i32 * 2;
            }

            let priority = ((object[2] >> 10) & 0x3) as i8 - 1;

            if type_ == 3 {
                todo!();
                let (data_base_addr, bitmap_width) =
                    if bool::from(self.inner.disp_cnt.bitmap_obj_mapping()) {
                        (
                            (object[2] as u32 & 0x3FF)
                                * if bool::from(self.inner.disp_cnt.bitmap_obj_1d_boundary()) {
                                    256
                                } else {
                                    128
                                },
                            width,
                        )
                    } else {
                        let mask = if bool::from(self.inner.disp_cnt.bitmap_obj_2d()) {
                            0x1F
                        } else {
                            0xF
                        };
                        (
                            (object[2] as u32 & mask) * 0x10
                                + (object[2] as u32 & 0x3FF & !mask) * 0x80,
                            if bool::from(self.inner.disp_cnt.bitmap_obj_2d()) {
                                256
                            } else {
                                128
                            },
                        )
                    };
                let data_base_addr = Self::get_obj_offset() + data_base_addr;

                if object[0] & (1 << 8) != 0 {
                    let params_base_addr = ((object[1] >> 9) & 0x1F) as u32 * 0x20;
                    let params = [
                        self.read_oam::<u16>(params_base_addr + 0x6, mem) as i16,
                        self.read_oam::<u16>(params_base_addr + 0xE, mem) as i16,
                        self.read_oam::<u16>(params_base_addr + 0x16, mem) as i16,
                        self.read_oam::<u16>(params_base_addr + 0x1E, mem) as i16,
                    ];

                    for j in 0..width2 {
                        let offset = x + j as i32;
                        if offset < 0 || offset >= DISPLAY_WIDTH as i32 {
                            continue;
                        }

                        let rot_scale_x = ((params[0] as i32 * (j as i32 - (width2 as i32 >> 1))
                            + params[1] as i32 * (sprite_y - (height2 as i32 >> 1)))
                            >> 8)
                            + (width2 as i32 >> 1);
                        if rot_scale_x < 0 || rot_scale_x >= width as i32 {
                            continue;
                        }

                        let rot_scale_y = ((params[2] as i32 * (j as i32 - (width2 as i32 >> 1))
                            + params[3] as i32 * (sprite_y - (height2 as i32 >> 1)))
                            >> 8)
                            + (height as i32 >> 1);
                        if rot_scale_y < 0 || rot_scale_y >= height as i32 {
                            continue;
                        }

                        let pixel = mem.vram.read::<{ ARM9 }, u16>(
                            data_base_addr
                                + ((rot_scale_y as u32 * bitmap_width + rot_scale_x as u32) << 1),
                        );
                        if pixel * (1 << 15) != 0 {
                            self.draw_obj_pixel(line, offset as usize, pixel as u32, priority);
                        }
                    }
                } else {
                    let data_base_addr = (data_base_addr as i32
                        + if object[1] & (1 << 13) != 0 {
                            height as i32 - sprite_y - 1
                        } else {
                            sprite_y
                        } * (bitmap_width << 1) as i32)
                        as u32;

                    for j in 0..width {
                        let offset = if object[1] & (1 << 12) != 0 {
                            x + width as i32 - j as i32 - 1
                        } else {
                            x + j as i32
                        };
                        if offset < 0 || offset >= DISPLAY_WIDTH as i32 {
                            continue;
                        }

                        let pixel = mem.vram.read::<{ ARM9 }, u16>(data_base_addr + (j << 1));
                        if pixel & (1 << 15) != 0 {
                            self.draw_obj_pixel(line, offset as usize, pixel as u32, priority);
                        }
                    }
                }

                continue;
            }

            let tile_base_addr = (object[2] as u32 & 0x3FF) * bound;

            if object[0] & (1 << 8) != 0 {
                let params_base_addr = ((object[1] >> 9) & 0x1F) as u32 * 0x20;
                let params = [
                    self.read_oam::<u16>(params_base_addr + 0x6, mem) as i16,
                    self.read_oam::<u16>(params_base_addr + 0xE, mem) as i16,
                    self.read_oam::<u16>(params_base_addr + 0x16, mem) as i16,
                    self.read_oam::<u16>(params_base_addr + 0x1E, mem) as i16,
                ];

                if object[0] & (1 << 13) != 0 {
                    let map_width = if bool::from(self.inner.disp_cnt.tile_obj_mapping()) {
                        width
                    } else {
                        DISPLAY_WIDTH as u32 / 2
                    };

                    let palette_base_addr =
                        if bool::from(self.inner.disp_cnt.obj_extended_palettes()) {
                            ((object[2] & 0xF000) >> 3) as u32
                        } else {
                            Self::get_palettes_offset() + 0x200
                        };

                    for j in 0..width2 {
                        let offset = x + j as i32;
                        if offset < 0 || offset >= DISPLAY_WIDTH as i32 {
                            continue;
                        }

                        let rot_scale_x = ((params[0] as i32 * (j as i32 - (width2 as i32 >> 1))
                            + params[1] as i32 * (sprite_y - (height2 as i32 >> 1)))
                            >> 8)
                            + (width as i32 >> 1);
                        if rot_scale_x < 0 || rot_scale_x >= width as i32 {
                            continue;
                        }

                        let rot_scale_y = ((params[2] as i32 * (j as i32 - (width2 as i32 >> 1))
                            + params[3] as i32 * (sprite_y - (height2 as i32 >> 1)))
                            >> 8)
                            + (height as i32 >> 1);
                        if rot_scale_y < 0 || rot_scale_y >= width as i32 {
                            continue;
                        }
                        let rot_scale_x = rot_scale_x as u32;
                        let rot_scale_y = rot_scale_y as u32;

                        let index = self.read_obj::<u8>(
                            tile_base_addr
                                + (((rot_scale_y >> 3) * map_width + (rot_scale_y & 7)) << 3)
                                + ((rot_scale_x >> 3) << 6)
                                + (rot_scale_x & 7),
                            mem,
                        );

                        if index != 0 {
                            if type_ == 2 {
                                self.framebuffer[((line as usize) << 8) + offset as usize] |=
                                    1 << 24;
                            } else {
                                self.draw_obj_pixel(
                                    line,
                                    offset as usize,
                                    (((type_ == 1) as u32) << 25)
                                        | (1 << 15)
                                        | read_palette(
                                            mem,
                                            palette_base_addr + ((index as u32) << 1),
                                        ) as u32,
                                    priority,
                                );
                            }
                        }
                    }
                } else {
                    let map_width = if bool::from(self.inner.disp_cnt.tile_obj_mapping()) {
                        width
                    } else {
                        DISPLAY_WIDTH as u32
                    };

                    let palette_addr = 0x200 + (((object[2] as u32 & 0xF000) >> 12) << 5);

                    for j in 0..width2 {
                        let offset = x + j as i32;
                        if offset < 0 || offset >= DISPLAY_WIDTH as i32 {
                            continue;
                        }

                        let rot_scale_x = ((params[0] as i32 * (j as i32 - (width2 as i32 >> 1))
                            + params[1] as i32 * (sprite_y - (height2 as i32 >> 1)))
                            >> 8)
                            + (width as i32 >> 1);
                        if rot_scale_x < 0 || rot_scale_x >= width as i32 {
                            continue;
                        }

                        let rot_scale_y = ((params[2] as i32 * (j as i32 - (width2 as i32 >> 1))
                            + params[3] as i32 * (sprite_y - (height2 as i32 >> 1)))
                            >> 8)
                            + (height as i32 >> 1);
                        if rot_scale_y < 0 || rot_scale_y >= width as i32 {
                            continue;
                        }
                        let rot_scale_x = rot_scale_x as u32;
                        let rot_scale_y = rot_scale_y as u32;

                        let index = self.read_obj::<u8>(
                            tile_base_addr
                                + ((((rot_scale_y >> 3) * map_width + (rot_scale_y & 7)) << 3)
                                    << 2)
                                + ((rot_scale_x >> 3) << 5)
                                + ((rot_scale_x & 7) >> 1),
                            mem,
                        );
                        let index = if j & 1 != 0 {
                            (index & 0xF0) >> 4
                        } else {
                            index & 0xF
                        };

                        if index != 0 {
                            if type_ == 2 {
                                self.framebuffer[((line as usize) << 8) + offset as usize] |=
                                    1 << 24;
                            } else {
                                self.draw_obj_pixel(
                                    line,
                                    offset as usize,
                                    (((type_ == 1) as u32) << 25)
                                        | (1 << 15)
                                        | self.read_palettes::<u16>(
                                            palette_addr + ((index as u32) << 1),
                                            mem,
                                        ) as u32,
                                    priority,
                                );
                            }
                        }
                    }
                }
            } else if object[0] & (1 << 13) != 0 {
                let map_width = if bool::from(self.inner.disp_cnt.tile_obj_mapping()) {
                    width
                } else {
                    DISPLAY_WIDTH as u32 / 2
                };
                let sprite_y = sprite_y as u32;
                let tile_base_addr = tile_base_addr
                    + if object[1] & (1 << 13) != 0 {
                        (7 - (sprite_y & 7) + ((height - 1 - sprite_y) >> 3) * map_width) << 3
                    } else {
                        ((sprite_y & 7) + (sprite_y >> 3) * map_width) << 3
                    };

                let palette_base_addr = if bool::from(self.inner.disp_cnt.obj_extended_palettes()) {
                    if !mem.vram.is_obj_ext_palette_mapped::<ENGINE>() {
                        continue;
                    }
                    ((object[2] & 0xF000) >> 3) as u32
                } else {
                    Self::get_palettes_offset() + 0x200
                };

                for j in 0..width {
                    let offset = if object[1] & (1 << 12) != 0 {
                        x + width as i32 - j as i32 - 1
                    } else {
                        x + j as i32
                    };
                    if offset < 0 || offset >= DISPLAY_WIDTH as i32 {
                        continue;
                    }

                    let index =
                        self.read_obj::<u8>(tile_base_addr + ((j >> 3) << 6) + (j & 7), mem);
                    let index = if j & 1 != 0 {
                        (index & 0xF0) >> 4
                    } else {
                        index & 0xF
                    };

                    if index != 0 {
                        if type_ == 2 {
                            self.framebuffer[((line as usize) << 8) + offset as usize] |= 1 << 24;
                        } else {
                            self.draw_obj_pixel(
                                line,
                                offset as usize,
                                (((type_ == 1) as u32) << 25)
                                    | (1 << 15)
                                    | read_palette(mem, palette_base_addr + ((index as u32) << 1))
                                        as u32,
                                priority,
                            );
                        }
                    }
                }
            } else {
                let map_width = if bool::from(self.inner.disp_cnt.tile_obj_mapping()) {
                    width
                } else {
                    DISPLAY_WIDTH as u32
                };
                let sprite_y = sprite_y as u32;
                let tile_base_addr = tile_base_addr
                    + if object[1] & (1 << 13) != 0 {
                        (7 - (sprite_y & 7) + ((height - 1 - sprite_y) >> 3) * map_width) << 2
                    } else {
                        ((sprite_y & 7) + (sprite_y >> 3) * map_width) << 2
                    };

                let palette_addr = 0x200 + (((object[2] as u32 & 0xF000) >> 12) << 5);

                for j in 0..width {
                    let offset = if object[1] & (1 << 12) != 0 {
                        x + width as i32 - j as i32 - 1
                    } else {
                        x + j as i32
                    };
                    if offset < 0 || offset >= DISPLAY_WIDTH as i32 {
                        continue;
                    }

                    let index =
                        self.read_obj::<u8>(tile_base_addr + ((j >> 3) << 5) + ((j & 7) >> 1), mem);
                    let index = if j & 1 != 0 {
                        (index & 0xF0) >> 4
                    } else {
                        index & 0xF
                    };

                    if index != 0 {
                        if type_ == 2 {
                            self.framebuffer[((line as usize) << 8) + offset as usize] |= 1 << 24;
                        } else {
                            self.draw_obj_pixel(
                                line,
                                offset as usize,
                                (((type_ == 1) as u32) << 25)
                                    | (1 << 15)
                                    | self.read_palettes::<u16>(
                                        palette_addr + ((index as u32) << 1),
                                        mem,
                                    ) as u32,
                                priority,
                            );
                        }
                    }
                }
            }
        }
    }

    fn draw_bg_pixel<const BG: usize>(&mut self, line: u8, x: usize, pixel: u32) {
        let pixels_a = self.layers.get_pixels_mut::<{ Gpu2DLayer::A }>();
        let pixels_b = self.layers.get_pixels_mut::<{ Gpu2DLayer::B }>();
        let priorities_a = self.layers.get_priorities_mut::<{ Gpu2DLayer::A }>();
        let priorities_b = self.layers.get_priorities_mut::<{ Gpu2DLayer::B }>();
        let blend_bits_a = self.layers.get_blend_bits_mut::<{ Gpu2DLayer::A }>();
        let blend_bits_b = self.layers.get_blend_bits_mut::<{ Gpu2DLayer::B }>();

        if self.inner.disp_cnt.is_any_window_enabled() {
            // TODO
        }

        let bg_priority = u8::from(self.inner.bg_cnt[BG].priority()) as i8;
        unsafe {
            if bg_priority <= *priorities_a.get_unchecked(x) {
                *pixels_b.get_unchecked_mut(x) = *pixels_a.get_unchecked(x);
                *priorities_b.get_unchecked_mut(x) = *priorities_a.get_unchecked(x);
                *blend_bits_b.get_unchecked_mut(x) = *blend_bits_a.get_unchecked(x);

                *pixels_a.get_unchecked_mut(x) = pixel;
                *priorities_a.get_unchecked_mut(x) = bg_priority;
                *blend_bits_a.get_unchecked_mut(x) = BG as i8;
            } else if bg_priority <= *priorities_b.get_unchecked(x) {
                *pixels_b.get_unchecked_mut(x) = pixel;
                *priorities_b.get_unchecked_mut(x) = bg_priority;
                *blend_bits_b.get_unchecked_mut(x) = BG as i8;
            }
        }
    }

    fn draw_obj_pixel(&mut self, line: u8, x: usize, pixel: u32, priority: i8) {
        let pixels_a = self.layers.get_pixels_mut::<{ Gpu2DLayer::A }>();
        let priorities_a = self.layers.get_priorities_mut::<{ Gpu2DLayer::A }>();
        let blend_bits_a = self.layers.get_blend_bits_mut::<{ Gpu2DLayer::A }>();

        if self.inner.disp_cnt.is_any_window_enabled() {
            // TODO
        }

        unsafe {
            if pixels_a[x] & (1 << 15) == 0 || priority < *priorities_a.get_unchecked(x) {
                *pixels_a.get_unchecked_mut(x) = pixel;
                *priorities_a.get_unchecked_mut(x) = priority;
                *blend_bits_a.get_unchecked_mut(x) = 4;
            }
        }
    }

    fn rgb5_to_rgb6(color: u32) -> u32 {
        let r = (color & 0x1F) << 1;
        let g = ((color >> 5) & 0x1F) << 1;
        let b = ((color >> 10) & 0x1F) << 1;
        (color & 0xFFFC0000) | (b << 12) | (g << 6) | r
    }

    pub fn reload_registers(&mut self) {
        let internal = &mut self.inner.internal;
        internal.x[0] = self.inner.bg_x[0];
        internal.y[0] = self.inner.bg_y[0];
        internal.x[1] = self.inner.bg_x[1];
        internal.y[1] = self.inner.bg_y[1];
    }
}

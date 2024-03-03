use crate::hle::gpu::gpu_context::{DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH};
use crate::hle::memory::palettes_context::PalettesContext;
use crate::hle::memory::regions::VRAM_OFFSET;
use crate::hle::memory::vram_context;
use crate::hle::memory::vram_context::{VramContext, BG_A_OFFSET, BG_B_OFFSET};
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils::{HeapMemU32, HeapMemU8};
use bilge::prelude::*;
use std::cell::RefCell;
use std::hint::unreachable_unchecked;
use std::marker::ConstParamTy;
use std::mem;
use std::rc::Rc;

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
    priorities: HeapMemU8<{ DISPLAY_WIDTH * 2 }>,
    blend_bits: HeapMemU8<{ DISPLAY_WIDTH * 2 }>,
}

impl Gpu2DLayers {
    fn new() -> Self {
        let pixels = HeapMemU32::new();
        Gpu2DLayers {
            pixels,
            priorities: HeapMemU8::new(),
            blend_bits: HeapMemU8::new(),
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

    fn get_priorities_mut<const LAYER: Gpu2DLayer>(&mut self) -> &'static mut [u8; DISPLAY_WIDTH] {
        let ptr = match LAYER {
            Gpu2DLayer::A => self.priorities[..DISPLAY_WIDTH].as_mut_ptr(),
            Gpu2DLayer::B => self.priorities[DISPLAY_WIDTH..].as_mut_ptr(),
        };
        unsafe {
            (ptr as *mut [u8; DISPLAY_WIDTH])
                .as_mut()
                .unwrap_unchecked()
        }
    }

    fn get_blend_bits_mut<const LAYER: Gpu2DLayer>(&mut self) -> &'static mut [u8; DISPLAY_WIDTH] {
        let ptr = match LAYER {
            Gpu2DLayer::A => self.blend_bits[..DISPLAY_WIDTH].as_mut_ptr(),
            Gpu2DLayer::B => self.blend_bits[DISPLAY_WIDTH..].as_mut_ptr(),
        };
        unsafe {
            (ptr as *mut [u8; DISPLAY_WIDTH])
                .as_mut()
                .unwrap_unchecked()
        }
    }
}

pub struct Gpu2DContext<const ENGINE: Gpu2DEngine> {
    inner: Gpu2DInner,
    layers: Gpu2DLayers,
    pub framebuffer: HeapMemU32<{ DISPLAY_PIXEL_COUNT }>,
    vram_context: *const VramContext,
    palettes_context: *const PalettesContext,
}

impl<const ENGINE: Gpu2DEngine> Gpu2DContext<ENGINE> {
    const fn get_bg_offset() -> u32 {
        match ENGINE {
            Gpu2DEngine::A => BG_A_OFFSET,
            Gpu2DEngine::B => BG_B_OFFSET,
        }
    }

    const fn get_palattes_offset() -> u32 {
        match ENGINE {
            Gpu2DEngine::A => 0,
            Gpu2DEngine::B => 0x400,
        }
    }

    pub fn new(
        vram_context: Rc<RefCell<VramContext>>,
        palattes_context: Rc<RefCell<PalettesContext>>,
    ) -> Self {
        Gpu2DContext {
            inner: Gpu2DInner::default(),
            layers: Gpu2DLayers::new(),
            framebuffer: HeapMemU32::new(),
            vram_context: vram_context.as_ptr(),
            palettes_context: palattes_context.as_ptr(),
        }
    }

    pub fn get_disp_cnt(&self) -> u32 {
        self.inner.disp_cnt.into()
    }

    pub fn get_bg_cnt(&self, bg_num: usize) -> u16 {
        self.inner.bg_cnt[bg_num].into()
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

    pub fn set_win_in(&mut self, mask: u16, value: u16) {}

    pub fn set_win_out(&mut self, mask: u16, value: u16) {}

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

    pub fn draw_scanline(&mut self, line: u8) {
        let backdrop = unsafe { (*self.palettes_context).read::<u16>(Self::get_palattes_offset()) };
        let backdrop = backdrop & !(1 << 15);
        self.layers.pixels.fill(backdrop as u32);
        self.layers.priorities.fill(4);
        self.layers.blend_bits.fill(5);

        let disp_cnt = self.inner.disp_cnt;
        if bool::from(disp_cnt.screen_display_obj()) {
            if bool::from(disp_cnt.obj_window_display_flag()) {
                self.draw_objects::<true>(line);
            }
            self.draw_objects::<false>(line);
        }

        macro_rules! draw {
            ($bg3:expr, $bg2:expr, $bg1:expr, $bg0:expr) => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    $bg3;
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    $bg2;
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    $bg1;
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    $bg0;
                }
            };
        }

        match u8::from(disp_cnt.bg_mode()) {
            0 => {
                draw!(
                    self.draw_text::<3>(line),
                    self.draw_text::<2>(line),
                    self.draw_text::<1>(line),
                    self.draw_text::<0>(line)
                );
            }
            1 => {
                draw!(
                    self.draw_affine::<3>(line),
                    self.draw_text::<2>(line),
                    self.draw_text::<1>(line),
                    self.draw_text::<0>(line)
                );
            }
            2 => {
                draw!(
                    self.draw_affine::<3>(line),
                    self.draw_affine::<2>(line),
                    self.draw_text::<1>(line),
                    self.draw_text::<0>(line)
                );
            }
            3 => {
                draw!(
                    self.draw_extended::<3>(line),
                    self.draw_text::<2>(line),
                    self.draw_text::<1>(line),
                    self.draw_text::<0>(line)
                );
            }
            4 => {
                draw!(
                    self.draw_extended::<3>(line),
                    self.draw_affine::<2>(line),
                    self.draw_text::<1>(line),
                    self.draw_text::<0>(line)
                );
            }
            5 => {
                draw!(
                    self.draw_extended::<3>(line),
                    self.draw_extended::<2>(line),
                    self.draw_text::<1>(line),
                    self.draw_text::<0>(line)
                );
            }
            6 => {
                draw!({}, self.draw_large::<2>(line), {}, {});
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
                todo!()
            } else {
                pixels_a[i] = Self::rgb5_to_rgb6(value);
                if pixels_a[i] & (1 << 25) != 0 {
                    if bld_cnt_raw & (1 << (8 + blend_bits_b[i])) != 0 {
                        todo!()
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
                let base_addr = vram_context::LCDC_OFFSET
                    + vram_block * vram_context::BANK_A_SIZE as u32
                    + ((fb_start as u32) << 1);
                let vram_context = unsafe { self.vram_context.as_ref().unwrap_unchecked() };

                fb.iter_mut().enumerate().for_each(|(i, value)| {
                    *value = Self::rgb5_to_rgb6(
                        vram_context.read::<{ CpuType::ARM9 }, u16>(base_addr + ((i as u32) << 1))
                            as u32,
                    );
                });
            }
            DisplayMode::MainMemory => {
                todo!()
            }
        }
    }

    fn draw_affine<const BG: usize>(&mut self, line: u8) {
        todo!()
    }

    fn draw_text<const BG: usize>(&mut self, line: u8) {
        if BG == 0 && bool::from(self.inner.disp_cnt.bg0_3d()) {
            // TODO 2d
            return;
        }

        if bool::from(self.inner.bg_cnt[BG].color_palettes()) {
            self.draw_text_pixels::<BG, true>(line);
        } else {
            self.draw_text_pixels::<BG, false>(line);
        }
    }

    fn draw_text_pixels<const BG: usize, const BIT8: bool>(&mut self, line: u8) {
        let disp_cnt = self.inner.disp_cnt;
        let bg_cnt = self.inner.bg_cnt[BG];

        let mut tile_base_addr = Self::get_bg_offset()
            + (u32::from(disp_cnt.screen_base()) << 16)
            + (u32::from(bg_cnt.screen_base_block()) << 11);
        let index_base_addr = Self::get_bg_offset()
            + (u32::from(disp_cnt.char_base()) << 16)
            + (u32::from(bg_cnt.char_base_block()) << 14);

        let y_offset = (if bool::from(bg_cnt.mosaic()) && self.inner.mosaic != 0 {
            todo!()
        } else {
            line as u16
        } + self.inner.bg_v_ofs[BG])
            & 0x1FF;

        tile_base_addr += (y_offset as u32 & 0xF8) << 3;
        if y_offset >= 256 && (u8::from(bg_cnt.screen_size()) & 1) != 0 {
            // TODO
        }

        let vram_context = unsafe { self.vram_context.as_ref().unwrap_unchecked() };
        let palattes_context = unsafe { self.palettes_context.as_ref().unwrap_unchecked() };

        let mut palettes_base_addr = Self::get_palattes_offset();
        let read_palettes = if BIT8 && bool::from(disp_cnt.bg_extended_palettes()) {
            palettes_base_addr = 0;
            if BG < 2 && bool::from(bg_cnt.ext_palette_slot_display_area_overflow()) {
                if !vram_context.is_bg_ext_palette_mapped::<ENGINE>(BG + 2) {
                    return;
                }
                |vram_context: &VramContext, _: &PalettesContext, addr: u32| {
                    vram_context.read_bg_ext_palette::<ENGINE, u16>(BG + 2, addr)
                }
            } else {
                if !vram_context.is_bg_ext_palette_mapped::<ENGINE>(BG) {
                    return;
                }
                |vram_context: &VramContext, _: &PalettesContext, addr: u32| {
                    vram_context.read_bg_ext_palette::<ENGINE, u16>(BG, addr)
                }
            }
        } else {
            |_: &VramContext, palettes_context: &PalettesContext, addr: u32| {
                palettes_context.read(addr)
            }
        };

        for i in (0..DISPLAY_WIDTH as u32).step_by(8) {
            let x_offset = (i + self.inner.bg_h_ofs[BG] as u32) & 0x1FF;
            let tile_addr = tile_base_addr + ((x_offset & 0xF8) >> 2);

            if x_offset >= 256 && (u8::from(bg_cnt.screen_size()) & 2) != 0 {
                todo!()
            }

            let tile = vram_context.read::<{ CpuType::ARM9 }, u16>(tile_addr);
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
                vram_context.read::<{ CpuType::ARM9 }, u32>(index_addr) as u64
                    | ((vram_context.read::<{ CpuType::ARM9 }, u32>(index_addr + 4) as u64) << 32)
            } else {
                vram_context.read::<{ CpuType::ARM9 }, u32>(index_addr) as u64
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
                        vram_context,
                        palattes_context,
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

    fn draw_extended<const BG: usize>(&mut self, line: u8) {
        let mut rot_scale_x = self.inner.internal.x[BG - 2] - self.inner.bg_pa[BG - 2] as i32;
        let mut rot_scale_y = self.inner.internal.y[BG - 2] - self.inner.bg_pc[BG - 2] as i32;

        let bg_cnt = self.inner.bg_cnt[BG];
        let vram_context = unsafe { self.vram_context.as_ref().unwrap_unchecked() };
        let palattes_context = unsafe { self.palettes_context.as_ref().unwrap_unchecked() };

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

                    let pixel = vram_context.read::<{ CpuType::ARM9 }, u16>(
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

                    let index = vram_context.read::<{ CpuType::ARM9 }, u8>(
                        (base_data_addr as i32 + y * size_x + x) as u32,
                    );
                    if index != 0 {
                        let pixel = palattes_context.read::<u16>(index as u32 * 2) | (1 << 15);
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

    fn draw_large<const BG: usize>(&self, line: u8) {
        todo!()
    }

    fn draw_objects<const WINDOW: bool>(&self, line: u8) {}

    fn draw_bg_pixel<const BG: usize>(&mut self, line: u8, x: usize, pixel: u32) {
        let pixels_a = self.layers.get_pixels_mut::<{ Gpu2DLayer::A }>();
        let pixels_b = self.layers.get_pixels_mut::<{ Gpu2DLayer::B }>();
        let priorities_a = self.layers.get_priorities_mut::<{ Gpu2DLayer::A }>();
        let priorities_b = self.layers.get_priorities_mut::<{ Gpu2DLayer::B }>();
        let blend_bits_a = self.layers.get_blend_bits_mut::<{ Gpu2DLayer::A }>();
        let blend_bits_b = self.layers.get_blend_bits_mut::<{ Gpu2DLayer::B }>();

        if bool::from(self.inner.disp_cnt.window0_display_flag()) {
            todo!()
        }
        if bool::from(self.inner.disp_cnt.window1_display_flag()) {
            todo!()
        }
        if bool::from(self.inner.disp_cnt.obj_window_display_flag()) {
            todo!()
        }

        let bg_priority = u8::from(self.inner.bg_cnt[BG].priority());
        if bg_priority <= priorities_a[x] {
            pixels_b[x] = pixels_a[x];
            priorities_b[x] = priorities_a[x];
            blend_bits_b[x] = blend_bits_a[x];

            pixels_a[x] = pixel;
            priorities_a[x] = bg_priority;
            blend_bits_a[x] = BG as u8;
        } else if bg_priority <= priorities_b[x] {
            pixels_b[x] = pixel;
            priorities_b[x] = bg_priority;
            blend_bits_b[x] = BG as u8;
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

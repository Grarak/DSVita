use crate::core::graphics::gpu_2d::Gpu2DEngine;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::cmp::min;
use std::mem;

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
pub struct DispCnt {
    pub bg_mode: u3,
    pub bg0_3d: bool,
    pub tile_1d_obj_mapping: bool,
    pub bitmap_obj_2d: bool,
    pub bitmap_obj_mapping: bool,
    pub forced_blank: bool,
    pub screen_display_bg0: bool,
    pub screen_display_bg1: bool,
    pub screen_display_bg2: bool,
    pub screen_display_bg3: bool,
    pub screen_display_obj: bool,
    pub window0_display_flag: bool,
    pub window1_display_flag: bool,
    pub obj_window_display_flag: bool,
    pub display_mode: u2,
    pub vram_block: u2,
    pub tile_obj_1d_boundary: u2,
    pub bitmap_obj_1d_boundary: bool,
    pub obj_processing_during_h_blank: bool,
    pub char_base: u3,
    pub screen_base: u3,
    pub bg_extended_palettes: bool,
    pub obj_extended_palettes: bool,
}

impl Default for DispCnt {
    fn default() -> Self {
        DispCnt::from(0)
    }
}

impl DispCnt {
    pub fn is_any_window_enabled(&self) -> bool {
        self.window0_display_flag() || self.window1_display_flag() || self.obj_window_display_flag()
    }
}

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
pub struct BgCnt {
    pub priority: u2,
    pub char_base_block: u4,
    pub mosaic: bool,
    pub color_256_palettes: bool,
    pub screen_base_block: u5,
    pub ext_palette_slot_display_area_overflow: bool,
    pub screen_size: u2,
}

impl Default for BgCnt {
    fn default() -> Self {
        BgCnt::from(0)
    }
}

#[bitsize(16)]
#[derive(FromBits)]
pub struct BldCnt {
    bg0_1st_target_pixel: bool,
    bg1_1st_target_pixel: bool,
    bg2_1st_target_pixel: bool,
    bg3_1st_target_pixel: bool,
    obj_1st_target_pixel: bool,
    bd_1st_target_pixel: bool,
    pub color_special_effect: u2,
    bg0_2nc_target_pixel: bool,
    bg1_2nc_target_pixel: bool,
    bg2_2nc_target_pixel: bool,
    bg3_2nc_target_pixel: bool,
    obj_2nc_target_pixel: bool,
    bd_2nc_target_pixel: bool,
    not_used: u2,
}

#[derive(Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum BldMode {
    None = 0,
    AlphaBlending = 1,
    BrightnessIncrease = 2,
    BrightnessDecrease = 3,
}

impl From<u8> for BldMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= BldMode::BrightnessDecrease as u8);
        unsafe { mem::transmute(value) }
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

#[derive(Default)]
pub struct Gpu2DRegistersInner {
    x: [i32; 2],
    y: [i32; 2],
}

#[derive(Default)]
pub struct Gpu2DRegisters {
    pub engine: Gpu2DEngine,
    pub disp_cnt: DispCnt,
    pub bg_cnt: [BgCnt; 4],
    pub bg_h_ofs: [u16; 4],
    pub bg_v_ofs: [u16; 4],
    pub bg_pa: [i16; 2],
    pub bg_pb: [i16; 2],
    pub bg_pc: [i16; 2],
    pub bg_pd: [i16; 2],
    pub bg_x: [i32; 2],
    pub bg_y: [i32; 2],
    pub bg_x_dirty: bool,
    pub bg_y_dirty: bool,
    pub bld_cnt: u16,
    pub bld_alpha: u16,
    pub bld_y: u8,
    pub win_h: [u16; 2],
    pub win_v: [u16; 2],
    pub win_in: u16,
    pub win_out: u16,
    pub mosaic: u16,
    pub master_bright: u16,
    internal: Gpu2DRegistersInner,
}

impl Gpu2DRegisters {
    pub fn new(engine: Gpu2DEngine) -> Self {
        Gpu2DRegisters { engine, ..Gpu2DRegisters::default() }
    }

    pub fn get_disp_cnt(&self) -> u32 {
        self.disp_cnt.into()
    }

    pub fn get_bg_cnt(&self, bg_num: usize) -> u16 {
        self.bg_cnt[bg_num].into()
    }

    pub fn set_disp_cnt(&mut self, mut mask: u32, value: u32) {
        if self.engine == Gpu2DEngine::B {
            mask &= 0xC0B1FFF7;
        }
        self.disp_cnt = ((u32::from(self.disp_cnt) & !mask) | (value & mask)).into();
        debug_println!("GPU engine {:?} set disp cnt {:x} {}", self.engine, u32::from(self.disp_cnt), u8::from(self.disp_cnt.display_mode()),);
    }

    pub fn set_bg_cnt(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.bg_cnt[bg_num] = ((u16::from(self.bg_cnt[bg_num]) & !mask) | (value & mask)).into();
    }

    pub fn set_bg_h_ofs(&mut self, bg_num: usize, mut mask: u16, value: u16) {
        mask &= 0x01FF;
        self.bg_h_ofs[bg_num] = (self.bg_h_ofs[bg_num] & !mask) | (value & mask);
    }

    pub fn set_bg_v_ofs(&mut self, bg_num: usize, mut mask: u16, value: u16) {
        mask &= 0x01FF;
        self.bg_v_ofs[bg_num] = (self.bg_v_ofs[bg_num] & !mask) | (value & mask);
    }

    pub fn set_bg_pa(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.bg_pa[bg_num - 2] = ((self.bg_pa[bg_num - 2] as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_bg_pb(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.bg_pb[bg_num - 2] = ((self.bg_pb[bg_num - 2] as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_bg_pc(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.bg_pc[bg_num - 2] = ((self.bg_pc[bg_num - 2] as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_bg_pd(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.bg_pd[bg_num - 2] = ((self.bg_pd[bg_num - 2] as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_bg_x(&mut self, bg_num: usize, mut mask: u32, value: u32) {
        mask &= 0x0FFFFFFF;
        let mut bg_x = (self.bg_x[bg_num - 2] as u32 & !mask) | (value & mask);
        if bg_x & (1 << 27) != 0 {
            bg_x |= 0xF0000000;
        } else {
            bg_x &= !0xF0000000;
        }
        let bg_x = bg_x as i32;
        self.internal.x[bg_num - 2] = bg_x;
        self.bg_x[bg_num - 2] = bg_x;
        self.bg_x_dirty = true;
    }

    pub fn set_bg_y(&mut self, bg_num: usize, mut mask: u32, value: u32) {
        mask &= 0x0FFFFFFF;
        let mut bg_y = (self.bg_y[bg_num - 2] as u32 & !mask) | (value & mask);
        if bg_y & (1 << 27) != 0 {
            bg_y |= 0xF0000000;
        } else {
            bg_y &= !0xF0000000;
        }
        let bg_y = bg_y as i32;
        self.internal.y[bg_num - 2] = bg_y;
        self.bg_y[bg_num - 2] = bg_y;
        self.bg_y_dirty = true;
    }

    pub fn set_win_h(&mut self, win: usize, mask: u16, value: u16) {
        self.win_h[win] = (self.win_h[win] & !mask) | (value & mask);
    }

    pub fn set_win_v(&mut self, win: usize, mask: u16, value: u16) {
        self.win_v[win] = (self.win_v[win] & !mask) | (value & mask);
    }

    pub fn set_win_in(&mut self, mut mask: u16, value: u16) {
        mask &= 0x3F3F;
        self.win_in = (self.win_in & !mask) | (value & mask);
    }

    pub fn set_win_out(&mut self, mut mask: u16, value: u16) {
        mask &= 0x3F3F;
        self.win_out = (self.win_out & !mask) | (value & mask);
    }

    pub fn set_mosaic(&mut self, mask: u16, value: u16) {
        self.mosaic = (self.mosaic & !mask) | (value & mask);
    }

    pub fn set_bld_cnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0x3FFF;
        self.bld_cnt = (self.bld_cnt & !mask) | (value & mask);
    }

    pub fn set_bld_alpha(&mut self, mut mask: u16, value: u16) {
        mask &= 0x1F1F;
        let alpha = (self.bld_alpha & !mask) | (value & mask);
        let eva = min(alpha & 0x1F, 16);
        let evb = min(alpha >> 8, 16);
        self.bld_alpha = eva | (evb << 8);
    }

    pub fn set_bld_y(&mut self, value: u8) {
        self.bld_y = min(value & 0x1F, 16);
    }

    pub fn set_master_bright(&mut self, mut mask: u16, value: u16) {
        mask &= 0xC01F;
        self.master_bright = (self.master_bright & !mask) | (value & mask);
    }
}

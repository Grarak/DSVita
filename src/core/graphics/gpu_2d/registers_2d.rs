use crate::core::emu::Emu;
use crate::core::graphics::gpu_2d::Gpu2DEngine;
use bilge::prelude::*;
use std::cmp::min;
use std::{mem, ptr};

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
    pub char_base_block: u2,
    pub not_used: u2,
    pub mosaic: u1,
    pub color_256_palettes: bool,
    pub screen_base_block: u5,
    pub ext_palette_slot_display_area_overflow: u1,
    pub screen_size: u2,
}

impl Default for BgCnt {
    fn default() -> Self {
        BgCnt::from(0)
    }
}

#[bitsize(16)]
#[derive(FromBits)]
struct BldCnt {
    bg0_1st_target_pixel: bool,
    bg1_1st_target_pixel: bool,
    bg2_1st_target_pixel: bool,
    bg3_1st_target_pixel: bool,
    obj_1st_target_pixel: bool,
    bd_1st_target_pixel: bool,
    color_special_effect: u2,
    bg0_2nc_target_pixel: bool,
    bg1_2nc_target_pixel: bool,
    bg2_2nc_target_pixel: bool,
    bg3_2nc_target_pixel: bool,
    obj_2nc_target_pixel: bool,
    bd_2nc_target_pixel: bool,
    not_used: u2,
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
pub struct Gpu2DRegisters {
    pub engine: Gpu2DEngine,
    pub bg_x_dirty: bool,
    pub bg_y_dirty: bool,
}

impl Emu {
    pub fn gpu_2d_regs_b_set_disp_cnt(&mut self) {
        self.mem.io.arm9().gpu_2d_regs_b_disp_cnt.value &= 0xC0B1FFF7;
    }

    pub fn gpu_2d_regs_set_bg_h_ofs(&mut self, engine: Gpu2DEngine, bg_num: usize) {
        *self.mem.io.arm9().gpu_2d_regs_bg_h_ofs(engine, bg_num) &= 0x01FF;
    }

    pub fn gpu_2d_regs_set_bg_v_ofs(&mut self, engine: Gpu2DEngine, bg_num: usize) {
        *self.mem.io.arm9().gpu_2d_regs_bg_v_ofs(engine, bg_num) &= 0x01FF;
    }

    pub fn gpu_2d_regs_set_bg_x(&mut self, engine: Gpu2DEngine, bg_num: usize) {
        let mut bg_x = *self.mem.io.arm9().gpu_2d_regs_bg_x(engine, bg_num) & 0x0FFFFFFF;
        if bg_x & (1 << 27) != 0 {
            bg_x |= 0xF0000000;
        } else {
            bg_x &= !0xF0000000;
        }
        *self.mem.io.arm9().gpu_2d_regs_bg_x(engine, bg_num) = bg_x;
        self.gpu.gpu_2d_regs[engine].bg_x_dirty = true;
    }

    pub fn gpu_2d_regs_set_bg_y(&mut self, engine: Gpu2DEngine, bg_num: usize) {
        let mut bg_y = *self.mem.io.arm9().gpu_2d_regs_bg_y(engine, bg_num) & 0x0FFFFFFF;
        if bg_y & (1 << 27) != 0 {
            bg_y |= 0xF0000000;
        } else {
            bg_y &= !0xF0000000;
        }
        *self.mem.io.arm9().gpu_2d_regs_bg_y(engine, bg_num) = bg_y;
        self.gpu.gpu_2d_regs[engine].bg_y_dirty = true;
    }

    pub fn gpu_2d_regs_set_win_in(&mut self, engine: Gpu2DEngine) {
        *self.mem.io.arm9().gpu_2d_regs_win_in(engine) &= 0x3F3F;
    }

    pub fn gpu_2d_regs_set_win_out(&mut self, engine: Gpu2DEngine) {
        *self.mem.io.arm9().gpu_2d_regs_win_out(engine) &= 0x3F3F;
    }

    pub fn gpu_2d_regs_set_bld_cnt(&mut self, engine: Gpu2DEngine) {
        *self.mem.io.arm9().gpu_2d_regs_bld_cnt(engine) &= 0x3FFF;
    }

    pub fn gpu_2d_regs_set_bld_alpha(&mut self, engine: Gpu2DEngine) {
        *self.mem.io.arm9().gpu_2d_regs_bld_alpha(engine) &= 0x1F1F;
    }

    pub fn gpu_2d_regs_set_bld_y(&mut self, engine: Gpu2DEngine) {
        *self.mem.io.arm9().gpu_2d_regs_bld_y(engine) &= 0x1F;
    }

    pub fn gpu_2d_regs_set_master_bright(&mut self, engine: Gpu2DEngine) {
        *self.mem.io.arm9().gpu_2d_regs_master_bright(engine) &= 0xC01F;
    }
}

impl Gpu2DRegisters {
    pub fn new(engine: Gpu2DEngine) -> Self {
        Gpu2DRegisters { engine, ..Gpu2DRegisters::default() }
    }
}

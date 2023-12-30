use crate::logging::debug_println;
use crate::utils::HeapMem;
use bilge::prelude::*;
use std::marker::ConstParamTy;
use std::mem;

#[bitsize(32)]
#[derive(FromBits)]
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

#[repr(u8)]
enum DisplayMode {
    Off,
    Layers,
    Vram,
    MainMemory,
}

impl From<u8> for DisplayMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= DisplayMode::MainMemory as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(ConstParamTy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Gpu2DEngine {
    A,
    B,
}

const DISPLAY_WIDTH: u32 = 256;
const DISPLAY_HEIGHT: u32 = 192;

pub struct Gpu2DContext<const ENGINE: Gpu2DEngine> {
    disp_cnt: u32,
    disp_stat: u16,
    pow_cnt1: u16,
    framebuffer: HeapMem<u32, { (DISPLAY_WIDTH * DISPLAY_HEIGHT) as usize }>,
}

impl<const ENGINE: Gpu2DEngine> Gpu2DContext<ENGINE> {
    pub fn new() -> Self {
        Gpu2DContext {
            disp_cnt: 0,
            disp_stat: 0,
            pow_cnt1: 0,
            framebuffer: HeapMem::new(),
        }
    }

    pub fn set_disp_cnt(&mut self, mut mask: u32, value: u32) {
        if ENGINE == Gpu2DEngine::B {
            mask &= 0xC0B1FFF7;
        }
        self.disp_cnt = (self.disp_cnt & !mask) | (value & mask);
    }

    pub fn set_bg_cnt(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_h_ofs(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_v_ofs(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_p_a(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_p_b(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_p_c(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_p_d(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_x(&mut self, _: usize, mask: u32, value: u32) {}

    pub fn set_bg_y(&mut self, _: usize, mask: u32, value: u32) {}

    pub fn set_win_h(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_win_v(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_win_in(&mut self, mask: u16, value: u16) {}

    pub fn set_win_out(&mut self, mask: u16, value: u16) {}

    pub fn set_mosaic(&mut self, mask: u16, value: u16) {}

    pub fn set_bld_cnt(&mut self, mask: u16, value: u16) {}

    pub fn set_bld_alpha(&mut self, mask: u16, value: u16) {}

    pub fn set_bld_y(&mut self, value: u8) {}

    pub fn set_master_bright(&mut self, mask: u16, value: u16) {}

    pub fn draw_scanline(&mut self, line: u16) {
        let disp_cnt = DispCnt::from(self.disp_cnt);
        if bool::from(disp_cnt.screen_display_obj()) {
            if bool::from(disp_cnt.obj_window_display_flag()) {
                self.draw_objects::<true>(line);
            }
            self.draw_objects::<false>(line);
        }

        match u8::from(disp_cnt.bg_mode()) {
            0 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_text::<3>(line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_text::<2>(line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(line);
                }
            }
            1 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_affine::<3>(line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_text::<2>(line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(line);
                }
            }
            2 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_affine::<3>(line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_affine::<2>(line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(line);
                }
            }
            3 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_extended::<3>(line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_text::<2>(line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(line);
                }
            }
            4 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_extended::<3>(line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_affine::<2>(line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(line);
                }
            }
            5 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_extended::<3>(line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_extended::<2>(line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(line);
                }
            }
            6 => {
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_large::<2>(line);
                }
            }
            _ => {
                debug_println!("Unknown engine {:?} bg mode {}", ENGINE, disp_cnt.bg_mode());
            }
        }

        match DisplayMode::from(u8::from(disp_cnt.display_mode())) {
            DisplayMode::Off => {
                self.framebuffer[line as usize * DISPLAY_WIDTH as usize
                    ..(line + 1) as usize * DISPLAY_WIDTH as usize]
                    .fill(!0);
            }
            DisplayMode::Layers => {
                todo!()
            }
            DisplayMode::Vram => {
                todo!()
            }
            DisplayMode::MainMemory => {
                debug_println!("Unimplemented engine {:?} main memory mode", ENGINE);
            }
        }
    }

    fn draw_affine<const BG: u8>(&self, line: u16) {
        todo!()
    }

    fn draw_text<const BG: u8>(&self, line: u16) {
        todo!()
    }

    fn draw_extended<const BG: u8>(&self, line: u16) {
        todo!()
    }

    fn draw_large<const BG: u8>(&self, line: u16) {
        todo!()
    }

    fn draw_objects<const WINDOW: bool>(&self, line: u16) {
        todo!()
    }
}

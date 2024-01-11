use crate::hle::gpu::gpu_context::{DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH};
use crate::hle::memory::palettes_context::PalettesContext;
use crate::hle::memory::vram_context::{VramContext, BG_A_OFFSET, BG_B_OFFSET};
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils::{FastCell, HeapMemU32};
use bilge::prelude::*;
use std::marker::ConstParamTy;
use std::mem;
use std::rc::Rc;
use std::sync::Arc;

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
#[derive(FromBits)]
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

pub struct Gpu2DContext<const ENGINE: Gpu2DEngine> {
    pub disp_cnt: u32,
    pub bg_cnt: [u16; 4],
    bg_h_ofs: [u16; 4],
    bg_v_ofs: [u16; 4],
    disp_stat: u16,
    pow_cnt1: u16,
    pub framebuffer: HeapMemU32<{ DISPLAY_PIXEL_COUNT }>,
    layers: [FastCell<HeapMemU32<{ DISPLAY_WIDTH }>>; 2],
    vram_context: Arc<VramContext>,
    palattes_context: Rc<FastCell<PalettesContext>>,
}

impl<const ENGINE: Gpu2DEngine> Gpu2DContext<ENGINE> {
    pub fn new(
        vram_context: Arc<VramContext>,
        palattes_context: Rc<FastCell<PalettesContext>>,
    ) -> Self {
        Gpu2DContext {
            disp_cnt: 0,
            bg_cnt: [0u16; 4],
            bg_h_ofs: [0u16; 4],
            bg_v_ofs: [0u16; 4],
            disp_stat: 0,
            pow_cnt1: 0,
            framebuffer: HeapMemU32::new(),
            layers: [
                FastCell::new(HeapMemU32::new()),
                FastCell::new(HeapMemU32::new()),
            ],
            vram_context,
            palattes_context,
        }
    }

    pub fn set_disp_cnt(&mut self, mut mask: u32, value: u32) {
        if ENGINE == Gpu2DEngine::B {
            mask &= 0xC0B1FFF7;
        }
        self.disp_cnt = (self.disp_cnt & !mask) | (value & mask);
        debug_println!("GPU engine {:?} set disp cnt {:x}", ENGINE, self.disp_cnt);
    }

    pub fn set_bg_cnt(&mut self, bg_num: usize, mask: u16, value: u16) {
        self.bg_cnt[bg_num] = (self.bg_cnt[bg_num] & !mask) | (value & value);
    }

    pub fn set_bg_h_ofs(&mut self, bg_num: usize, mut mask: u16, value: u16) {
        mask &= 0x01FF;
        self.bg_h_ofs[bg_num] = (self.bg_h_ofs[bg_num] & !mask) | (value & mask);
    }

    pub fn set_bg_v_ofs(&mut self, bg_num: usize, mut mask: u16, value: u16) {
        mask &= 0x01FF;
        self.bg_v_ofs[bg_num] = (self.bg_v_ofs[bg_num] & !mask) | (value & mask);
    }

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
        let mut backdrop = [0u16; 1];
        self.palattes_context.borrow().read_slice(
            match ENGINE {
                Gpu2DEngine::A => 0,
                Gpu2DEngine::B => 0x400,
            },
            &mut backdrop,
        );
        backdrop[0] &= 1 << 15;
        for layers in &mut self.layers {
            layers.borrow_mut().fill(backdrop[0] as u32);
        }

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
            7 => {
                todo!()
            }
            _ => {
                debug_println!("Unknown engine {:?} bg mode {}", ENGINE, disp_cnt.bg_mode());
            }
        }

        let mut layer_a = self.layers[0].borrow_mut();
        let layer_b = self.layers[1].borrow().as_ptr() as u32;
        layer_a.iter_mut().enumerate().for_each(|(index, value)| {
            let layer_b = unsafe { (layer_b as *const [u32; 256]).as_ref().unwrap() };
            if *value & (1 << 26) != 0 {
                todo!()
            } else {
                *value = Self::rgb5_to_rgb6(*value);
            }
        });

        let fb_start = line as usize * DISPLAY_WIDTH;
        let fb_end = fb_start + DISPLAY_WIDTH;
        match DisplayMode::from(u8::from(disp_cnt.display_mode())) {
            DisplayMode::Off => {
                self.framebuffer[fb_start..fb_end].fill(!0);
            }
            DisplayMode::Layers => {
                self.framebuffer[fb_start..fb_end].copy_from_slice(layer_a.as_slice());
            }
            DisplayMode::Vram => {
                todo!()
            }
            DisplayMode::MainMemory => {
                todo!()
            }
        }
    }

    fn draw_affine<const BG: usize>(&self, line: u16) {
        todo!()
    }

    fn draw_text<const BG: usize>(&mut self, line: u16) {
        let disp_cnt = DispCnt::from(self.disp_cnt);
        if BG == 0 && bool::from(disp_cnt.bg0_3d()) {
            todo!()
        }
        let bgcnt = BgCnt::from(self.bg_cnt[BG]);

        let vram_offset = if ENGINE == Gpu2DEngine::A {
            BG_A_OFFSET
        } else {
            BG_B_OFFSET
        };

        let mut tile_base_addr = vram_offset
            + u32::from(disp_cnt.screen_base()) * 64 * 1024
            + u32::from(bgcnt.screen_base_block()) * 2 * 1024;
        let index_base_addr = vram_offset
            + u32::from(disp_cnt.char_base()) * 64 * 1024
            + u32::from(bgcnt.char_base_block()) * 16 * 1024;

        let y_offset = (if bool::from(bgcnt.mosaic()) {
            todo!()
        } else {
            line
        } + self.bg_v_ofs[BG])
            & 0x1FF;
        tile_base_addr += (y_offset as u32 & 0xF8) << 3;

        if y_offset >= 256 && (u8::from(bgcnt.screen_size()) & 1) != 0 {
            todo!()
        }

        if bool::from(bgcnt.color_palettes()) {
            todo!()
        } else {
            for i in (0..256).step_by(8) {
                let x_offset = (i + self.bg_h_ofs[BG]) & 0x1FF;
                let tile_addr = tile_base_addr + ((x_offset as u32 & 0xF8) >> 2);

                if x_offset >= 256 && (u8::from(bgcnt.screen_size()) & 2) != 0 {
                    todo!()
                }

                let mut tile = [0u16; 1];
                self.vram_context
                    .read_slice::<{ CpuType::ARM9 }, _>(tile_addr, &mut tile);
                let tile = TextBgScreen::from(tile[0]);

                let palette_base_addr = u32::from(tile.palette_num()) * 32;

                let index_addr = index_base_addr
                    + u32::from(tile.tile_num()) * 32
                    + if bool::from(tile.v_flip()) {
                        7 - ((y_offset as u32) % 8)
                    } else {
                        y_offset as u32 % 8
                    } * 4;
                let mut indices = [0u32; 1];
                self.vram_context
                    .read_slice::<{ CpuType::ARM9 }, _>(index_addr, &mut indices);
                let mut indices = indices[0];

                let mut x = i.wrapping_sub(x_offset % 8);
                while indices != 0 {
                    let tmp_x = if bool::from(tile.h_flip()) { 7 - x } else { x };
                    if tmp_x < 256 && (indices & 0xF) != 0 {
                        let mut color = [0u16; 1];
                        self.palattes_context
                            .borrow()
                            .read_slice(palette_base_addr + (indices & 0xF) * 2, &mut color);
                        self.draw_pixel::<BG>(line, tmp_x, color[0] | (1 << 15));
                    }
                    x = x.wrapping_add(1);
                    indices >>= 4;
                }
            }
        }
    }

    fn draw_extended<const BG: usize>(&self, line: u16) {
        todo!()
    }

    fn draw_large<const BG: usize>(&self, line: u16) {
        todo!()
    }

    fn draw_objects<const WINDOW: bool>(&self, line: u16) {
        todo!()
    }

    fn draw_pixel<const BG: usize>(&mut self, line: u16, x: u16, pixel: u16) {
        todo!()
    }

    fn rgb5_to_rgb6(color: u32) -> u32 {
        let r = (color & 0x1F) << 1;
        let g = (color & (0x1F << 5)) << 7;
        let b = (color & (0x1F << 10)) << 13;
        (color & 0xFFFC0000) | r | b | g
    }
}

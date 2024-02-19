use crate::hle::gpu::gpu_context::{DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH};
use crate::hle::memory::palettes_context::PalettesContext;
use crate::hle::memory::regions::VRAM_OFFSET;
use crate::hle::memory::vram_context;
use crate::hle::memory::vram_context::{VramContext, BG_A_OFFSET, BG_B_OFFSET};
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils::HeapMemU32;
use bilge::prelude::*;
use std::cell::RefCell;
use std::hint::unreachable_unchecked;
use std::marker::ConstParamTy;
use std::mem;
use std::ops::DerefMut;
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

struct Gpu2DInner<const ENGINE: Gpu2DEngine> {
    disp_cnt: u32,
    bg_cnt: [u16; 4],
    bg_h_ofs: [u16; 4],
    bg_v_ofs: [u16; 4],
    bg_pa: [i16; 2],
    bg_pb: [i16; 2],
    bg_pc: [i16; 2],
    bg_pd: [i16; 2],
    bg_x: [i32; 2],
    bg_y: [i32; 2],
    mosaic: u16,
    disp_stat: u16,
    pow_cnt1: u16,
    internal_x: [i32; 2],
    internal_y: [i32; 2],
}

impl<const ENGINE: Gpu2DEngine> Gpu2DInner<ENGINE> {
    fn new() -> Self {
        Gpu2DInner {
            disp_cnt: 0,
            bg_cnt: [0u16; 4],
            bg_h_ofs: [0u16; 4],
            bg_v_ofs: [0u16; 4],
            bg_pa: [0; 2],
            bg_pb: [0; 2],
            bg_pc: [0; 2],
            bg_pd: [0; 2],
            bg_x: [0; 2],
            bg_y: [0; 2],
            mosaic: 0,
            disp_stat: 0,
            pow_cnt1: 0,
            internal_x: [0; 2],
            internal_y: [0; 2],
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
        self.bg_cnt[bg_num] = (self.bg_cnt[bg_num] & !mask) | (value & mask);
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
        self.internal_x[bg_num - 2] = bg_x;
        self.bg_x[bg_num - 2] = bg_x;
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
        self.internal_y[bg_num - 2] = bg_y;
        self.bg_y[bg_num - 2] = bg_y;
    }

    pub fn set_win_h(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_win_v(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_win_in(&mut self, mask: u16, value: u16) {}

    pub fn set_win_out(&mut self, mask: u16, value: u16) {}

    pub fn set_mosaic(&mut self, mask: u16, value: u16) {
        self.mosaic = (self.mosaic & !mask) | (value & mask);
    }

    pub fn set_bld_cnt(&mut self, mask: u16, value: u16) {}

    pub fn set_bld_alpha(&mut self, mask: u16, value: u16) {}

    pub fn set_bld_y(&mut self, value: u8) {}

    pub fn set_master_bright(&mut self, mask: u16, value: u16) {}
}

pub struct Gpu2DContext<const ENGINE: Gpu2DEngine> {
    inner: RefCell<Gpu2DInner<ENGINE>>,
    pub framebuffer: RefCell<HeapMemU32<{ DISPLAY_PIXEL_COUNT }>>,
    layers: [RefCell<HeapMemU32<{ DISPLAY_WIDTH }>>; 2],
    vram_context: *const VramContext,
    palattes_context: *const PalettesContext,
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
            inner: RefCell::new(Gpu2DInner::new()),
            framebuffer: RefCell::new(HeapMemU32::new()),
            layers: [
                RefCell::new(HeapMemU32::new()),
                RefCell::new(HeapMemU32::new()),
            ],
            vram_context: vram_context.as_ptr(),
            palattes_context: palattes_context.as_ptr(),
        }
    }

    pub fn get_disp_cnt(&self) -> u32 {
        self.inner.borrow().disp_cnt
    }

    pub fn get_bg_cnt(&self, bg_num: usize) -> u16 {
        self.inner.borrow().bg_cnt[bg_num]
    }

    pub fn set_disp_cnt(&self, mask: u32, value: u32) {
        self.inner.borrow_mut().set_disp_cnt(mask, value);
    }

    pub fn set_bg_cnt(&self, bg_num: usize, mask: u16, value: u16) {
        self.inner.borrow_mut().set_bg_cnt(bg_num, mask, value);
    }

    pub fn set_bg_h_ofs(&self, bg_num: usize, mask: u16, value: u16) {
        self.inner.borrow_mut().set_bg_h_ofs(bg_num, mask, value);
    }

    pub fn set_bg_v_ofs(&self, bg_num: usize, mask: u16, value: u16) {
        self.inner.borrow_mut().set_bg_v_ofs(bg_num, mask, value);
    }

    pub fn set_bg_pa(&self, bg_num: usize, mask: u16, value: u16) {
        self.inner.borrow_mut().set_bg_pa(bg_num, mask, value);
    }

    pub fn set_bg_pb(&self, bg_num: usize, mask: u16, value: u16) {
        self.inner.borrow_mut().set_bg_pb(bg_num, mask, value);
    }

    pub fn set_bg_pc(&self, bg_num: usize, mask: u16, value: u16) {
        self.inner.borrow_mut().set_bg_pc(bg_num, mask, value);
    }

    pub fn set_bg_pd(&self, bg_num: usize, mask: u16, value: u16) {
        self.inner.borrow_mut().set_bg_pd(bg_num, mask, value);
    }

    pub fn set_bg_x(&self, bg_num: usize, mask: u32, value: u32) {
        self.inner.borrow_mut().set_bg_x(bg_num, mask, value);
    }

    pub fn set_bg_y(&self, bg_num: usize, mask: u32, value: u32) {
        self.inner.borrow_mut().set_bg_y(bg_num, mask, value);
    }

    pub fn set_win_h(&self, bg_num: usize, mask: u16, value: u16) {
        self.inner.borrow_mut().set_win_h(bg_num, mask, value);
    }

    pub fn set_win_v(&self, bg_num: usize, mask: u16, value: u16) {
        self.inner.borrow_mut().set_win_v(bg_num, mask, value);
    }

    pub fn set_win_in(&self, mask: u16, value: u16) {
        self.inner.borrow_mut().set_win_in(mask, value);
    }

    pub fn set_win_out(&self, mask: u16, value: u16) {
        self.inner.borrow_mut().set_win_out(mask, value);
    }

    pub fn set_mosaic(&self, mask: u16, value: u16) {
        self.inner.borrow_mut().set_mosaic(mask, value);
    }

    pub fn set_bld_cnt(&self, mask: u16, value: u16) {
        self.inner.borrow_mut().set_bld_cnt(mask, value);
    }

    pub fn set_bld_alpha(&self, mask: u16, value: u16) {
        self.inner.borrow_mut().set_bld_alpha(mask, value);
    }

    pub fn set_bld_y(&self, value: u8) {
        self.inner.borrow_mut().set_bld_y(value);
    }

    pub fn set_master_bright(&self, mask: u16, value: u16) {
        self.inner.borrow_mut().set_master_bright(mask, value);
    }

    pub fn draw_scanline(&self, line: u8) {
        let backdrop = unsafe { (*self.palattes_context).read::<u16>(Self::get_palattes_offset()) };
        let backdrop = backdrop & !(1 << 15);
        self.layers[0].borrow_mut().fill(backdrop as u32);
        self.layers[1].borrow_mut().fill(backdrop as u32);

        let mut inner = self.inner.borrow_mut();
        let inner = inner.deref_mut();

        let disp_cnt = DispCnt::from(inner.disp_cnt);
        if bool::from(disp_cnt.screen_display_obj()) {
            if bool::from(disp_cnt.obj_window_display_flag()) {
                self.draw_objects::<true>(line);
            }
            self.draw_objects::<false>(line);
        }

        match u8::from(disp_cnt.bg_mode()) {
            0 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_text::<3>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_text::<2>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(inner, line);
                }
            }
            1 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_affine::<3>(line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_text::<2>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(inner, line);
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
                    self.draw_text::<1>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(inner, line);
                }
            }
            3 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_extended::<3>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_text::<2>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(inner, line);
                }
            }
            4 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_extended::<3>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_affine::<2>(line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(inner, line);
                }
            }
            5 => {
                if bool::from(disp_cnt.screen_display_bg3()) {
                    self.draw_extended::<3>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_extended::<2>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg1()) {
                    self.draw_text::<1>(inner, line);
                }
                if bool::from(disp_cnt.screen_display_bg0()) {
                    self.draw_text::<0>(inner, line);
                }
            }
            6 => {
                if bool::from(disp_cnt.screen_display_bg2()) {
                    self.draw_large::<2>(line);
                }
            }
            7 => {
                debug_println!("Unknown engine {:?} bg mode {}", ENGINE, disp_cnt.bg_mode());
            }
            _ => {
                unsafe { unreachable_unchecked() };
            }
        }

        {
            let mut layers_a = self.layers[0].borrow_mut();
            let mut layers_b = self.layers[1].borrow_mut();
            for i in 0..DISPLAY_WIDTH {
                let value = layers_a[i];
                if value & (1 << 26) != 0 {
                    todo!()
                } else {
                    layers_a[i] = Self::rgb5_to_rgb6(value);
                }
            }
        }

        let fb_start = line as usize * DISPLAY_WIDTH;
        let fb_end = fb_start + DISPLAY_WIDTH;
        let fb = &mut self.framebuffer.borrow_mut()[fb_start..fb_end];
        match DisplayMode::from(u8::from(disp_cnt.display_mode())) {
            DisplayMode::Off => {
                fb.fill(!0);
            }
            DisplayMode::Layers => {
                fb.copy_from_slice(self.layers[0].borrow().as_slice());
            }
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

    fn draw_affine<const BG: usize>(&self, line: u8) {
        todo!()
    }

    fn draw_text<const BG: usize>(&self, inner: &Gpu2DInner<ENGINE>, line: u8) {
        let disp_cnt = DispCnt::from(inner.disp_cnt);
        if BG == 0 && bool::from(disp_cnt.bg0_3d()) {
            todo!()
        }
        let bg_cnt = BgCnt::from(inner.bg_cnt[BG]);

        let mut tile_base_addr = Self::get_bg_offset()
            + (u32::from(disp_cnt.screen_base()) << 16)
            + (u32::from(bg_cnt.screen_base_block()) << 11);
        let index_base_addr = Self::get_bg_offset()
            + (u32::from(disp_cnt.char_base()) << 16)
            + (u32::from(bg_cnt.char_base_block()) << 14);

        let y_offset = (if bool::from(bg_cnt.mosaic()) && inner.mosaic != 0 {
            todo!()
        } else {
            line as u16
        } + inner.bg_v_ofs[BG])
            & 0x1FF;

        tile_base_addr += (y_offset as u32 & 0xF8) << 3;
        if y_offset >= 256 && (u8::from(bg_cnt.screen_size()) & 1) != 0 {
            todo!()
        }

        let vram_context = unsafe { self.vram_context.as_ref().unwrap_unchecked() };
        let palattes_context = unsafe { self.palattes_context.as_ref().unwrap_unchecked() };
        let mut layers_a = self.layers[0].borrow_mut();
        let layers_a = layers_a.as_mut();
        let mut layers_b = self.layers[1].borrow_mut();
        let layers_b = layers_b.as_mut();

        if bool::from(bg_cnt.color_palettes()) {
            for i in (0..256).step_by(8) {
                let x_offset = (i + inner.bg_h_ofs[BG]) & 0x1FF;
                let tile_addr = tile_base_addr + ((x_offset as u32 & 0xF8) >> 2);

                if x_offset >= 256 && (u8::from(bg_cnt.screen_size()) & 2) != 0 {
                    todo!()
                }

                let tile = vram_context.read::<{ CpuType::ARM9 }, u16>(tile_addr);
                let tile = TextBgScreen::from(tile);

                if bool::from(disp_cnt.bg_extended_palettes()) {
                    todo!()
                }

                let palette_base_addr = Self::get_palattes_offset();

                let index_addr = index_base_addr
                    + (u32::from(tile.tile_num()) << 6)
                    + (if bool::from(tile.v_flip()) {
                        7 - (y_offset as u32 & 7)
                    } else {
                        y_offset as u32 & 7
                    } << 3);
                let mut indices = vram_context.read::<{ CpuType::ARM9 }, u32>(index_addr) as u64
                    | ((vram_context.read::<{ CpuType::ARM9 }, u32>(index_addr + 4) as u64) << 32);

                let mut x = i.wrapping_sub(x_offset & 7);
                while indices != 0 {
                    let tmp_x = if bool::from(tile.h_flip()) {
                        7u16.wrapping_sub(x)
                    } else {
                        x
                    };
                    if tmp_x < 256 && (indices & 0xF) != 0 {
                        let color = palattes_context
                            .read::<u16>(palette_base_addr + ((indices as u32 & 0xFF) << 1));
                        Self::draw_pixel::<BG>(
                            disp_cnt,
                            line,
                            tmp_x,
                            (color | (1 << 15)) as u32,
                            layers_a,
                            layers_b,
                        );
                    }
                    x = x.wrapping_add(1);
                    indices >>= 8;
                }
            }
        } else {
            for i in (0..256).step_by(8) {
                let x_offset = (i + inner.bg_h_ofs[BG]) & 0x1FF;
                let tile_addr = tile_base_addr + ((x_offset as u32 & 0xF8) >> 2);

                if x_offset >= 256 && (u8::from(bg_cnt.screen_size()) & 2) != 0 {
                    todo!()
                }

                let tile = vram_context.read::<{ CpuType::ARM9 }, u16>(tile_addr);
                let tile = TextBgScreen::from(tile);

                let palette_base_addr =
                    (u32::from(tile.palette_num()) << 5) + Self::get_palattes_offset();

                let index_addr = index_base_addr
                    + (u32::from(tile.tile_num()) << 5)
                    + (if bool::from(tile.v_flip()) {
                        7 - (y_offset as u32 & 7)
                    } else {
                        y_offset as u32 & 7
                    } << 2);
                let mut indices = vram_context.read::<{ CpuType::ARM9 }, u32>(index_addr);

                let mut x = i.wrapping_sub(x_offset & 7);
                while indices != 0 {
                    let tmp_x = if bool::from(tile.h_flip()) {
                        7u16.wrapping_sub(x)
                    } else {
                        x
                    };
                    if tmp_x < 256 && (indices & 0xF) != 0 {
                        let color = palattes_context
                            .read::<u16>(palette_base_addr + ((indices & 0xF) << 1));
                        Self::draw_pixel::<BG>(
                            disp_cnt,
                            line,
                            tmp_x,
                            (color | (1 << 15)) as u32,
                            layers_a,
                            layers_b,
                        );
                    }
                    x = x.wrapping_add(1);
                    indices >>= 4;
                }
            }
        }
    }

    fn draw_extended<const BG: usize>(&self, inner: &mut Gpu2DInner<ENGINE>, line: u8) {
        let mut rot_scale_x = inner.internal_x[BG - 2] - inner.bg_pa[BG - 2] as i32;
        let mut rot_scale_y = inner.internal_y[BG - 2] - inner.bg_pc[BG - 2] as i32;

        let bg_cnt = BgCnt::from(inner.bg_cnt[BG]);
        let vram_context = unsafe { self.vram_context.as_ref().unwrap_unchecked() };
        let palattes_context = unsafe { self.palattes_context.as_ref().unwrap_unchecked() };

        if bool::from(bg_cnt.color_palettes()) {
            let base_data_addr = VRAM_OFFSET + (u32::from(bg_cnt.screen_base_block()) << 11);

            let (size_x, size_y) = match u8::from(bg_cnt.screen_size()) {
                0 => (128, 128),
                1 => (256, 256),
                2 => (512, 256),
                3 => (512, 512),
                _ => unsafe { unreachable_unchecked() },
            };

            let disp_cnt = DispCnt::from(inner.disp_cnt);
            let mut layers_a = self.layers[0].borrow_mut();
            let mut layers_b = self.layers[1].borrow_mut();

            if u8::from(bg_cnt.char_base_block()) & 1 != 0 {
                for i in 0..DISPLAY_WIDTH {
                    rot_scale_x += inner.bg_pa[BG - 2] as i32;
                    rot_scale_y += inner.bg_pc[BG - 2] as i32;
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
                        Self::draw_pixel::<BG>(
                            disp_cnt,
                            line,
                            i as u16,
                            pixel as u32,
                            &mut layers_a,
                            &mut layers_b,
                        );
                    }
                }
            } else {
                for i in 0..DISPLAY_WIDTH {
                    rot_scale_x += inner.bg_pa[BG - 2] as i32;
                    rot_scale_y += inner.bg_pc[BG - 2] as i32;
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
                        Self::draw_pixel::<BG>(
                            disp_cnt,
                            line,
                            i as u16,
                            pixel as u32,
                            &mut layers_a,
                            &mut layers_b,
                        );
                    }
                }
            }
        } else {
            todo!()
        }

        inner.internal_x[BG - 2] += inner.bg_pb[BG - 2] as i32;
        inner.internal_y[BG - 2] += inner.bg_pd[BG - 2] as i32;
    }

    fn draw_large<const BG: usize>(&self, line: u8) {
        todo!()
    }

    fn draw_objects<const WINDOW: bool>(&self, line: u8) {
        todo!()
    }

    fn draw_pixel<const BG: usize>(
        disp_cnt: DispCnt,
        line: u8,
        x: u16,
        pixel: u32,
        layers_a: &mut [u32; DISPLAY_WIDTH],
        layers_b: &mut [u32; DISPLAY_WIDTH],
    ) {
        if bool::from(disp_cnt.window0_display_flag()) {
            todo!()
        }
        if bool::from(disp_cnt.window1_display_flag()) {
            todo!()
        }
        if bool::from(disp_cnt.obj_window_display_flag()) {
            todo!()
        }

        layers_a[x as usize] = pixel;
    }

    fn rgb5_to_rgb6(color: u32) -> u32 {
        let r = (color & 0x1F) << 1;
        let g = ((color >> 5) & 0x1F) << 1;
        let b = ((color >> 10) & 0x1F) << 1;
        (color & 0xFFFC0000) | (b << 12) | (g << 6) | r
    }

    pub fn reload_registers(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.internal_x[0] = inner.bg_x[0];
        inner.internal_y[0] = inner.bg_y[0];
        inner.internal_x[1] = inner.bg_x[1];
        inner.internal_y[1] = inner.bg_y[1];
    }
}

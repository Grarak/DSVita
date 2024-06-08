use crate::emu::gpu::gl::gpu_2d_renderer::GpuMemBuf;
use crate::emu::gpu::gpu::DISPLAY_WIDTH;
use crate::emu::gpu::gpu_2d::{Gpu2D, Gpu2DEngine};
use bilge::prelude::*;

#[bitsize(16)]
#[derive(FromBits)]
struct TextBgScreen {
    tile_num: u10,
    h_flip: u1,
    v_flip: u1,
    palette_num: u4,
}

impl<const ENGINE: Gpu2DEngine> Gpu2D<ENGINE> {
    pub(super) fn draw_text<const BG: usize>(&mut self, line: u8, mem: &GpuMemBuf) {
        if BG == 0 && self.inner.disp_cnt.bg0_3d() {
            // TODO 3d
            return;
        }

        if bool::from(self.inner.bg_cnt[BG].color_palettes()) {
            self.draw_text_pixels::<BG, true>(line, mem);
        } else {
            self.draw_text_pixels::<BG, false>(line, mem);
        }
    }

    pub(super) fn draw_text_pixels<const BG: usize, const BIT8: bool>(&mut self, line: u8, mem: &GpuMemBuf) {
        let disp_cnt = self.inner.disp_cnt;
        let bg_cnt = self.inner.bg_cnt[BG];

        let mut screen_base_addr = (u32::from(disp_cnt.screen_base()) << 16) + (u32::from(bg_cnt.screen_base_block()) << 11);
        let char_base_addr = (u32::from(disp_cnt.char_base()) << 16) + (u32::from(bg_cnt.char_base_block()) << 14);

        let y_offset = (if bool::from(bg_cnt.mosaic()) && self.inner.mosaic != 0 { todo!() } else { line as u16 } + self.inner.bg_v_ofs[BG]) & 0x1FF;

        screen_base_addr += (y_offset as u32 & 0xF8) << 3;
        if y_offset >= 256 && (u8::from(bg_cnt.screen_size()) & 2) != 0 {
            screen_base_addr += if u8::from(bg_cnt.screen_size()) & 1 != 0 { 0x1000 } else { 0x800 };
        }

        let read_palettes = if BIT8 && bool::from(disp_cnt.bg_extended_palettes()) {
            if BG < 2 && bool::from(bg_cnt.ext_palette_slot_display_area_overflow()) {
                if !Self::is_bg_ext_palette_mapped(BG + 2, mem) {
                    return;
                }
                |mem: &GpuMemBuf, addr: u32| Self::read_bg_ext_palette::<u16>(BG + 2, addr, mem)
            } else {
                if !Self::is_bg_ext_palette_mapped(BG, mem) {
                    return;
                }
                |mem: &GpuMemBuf, addr: u32| Self::read_bg_ext_palette::<u16>(BG, addr, mem)
            }
        } else {
            |mem: &GpuMemBuf, addr: u32| Self::read_palettes(addr, mem)
        };

        for i in (0..=DISPLAY_WIDTH as u32).step_by(8) {
            let x_offset = (i + self.inner.bg_h_ofs[BG] as u32) & 0x1FF;
            let mut screen_addr = screen_base_addr + ((x_offset & 0xF8) >> 2);

            if x_offset >= 256 && (u8::from(bg_cnt.screen_size()) & 1) != 0 {
                screen_addr += 0x800;
            }

            let screen_entry = Self::read_bg::<u16>(screen_addr, mem);
            let screen_entry = TextBgScreen::from(screen_entry);

            let palette_addr = if BIT8 {
                if bool::from(disp_cnt.bg_extended_palettes()) {
                    u32::from(screen_entry.palette_num()) << 9
                } else {
                    0
                }
            } else {
                u32::from(screen_entry.palette_num()) << 5
            };

            let char_index_addr = char_base_addr
                + if BIT8 {
                    (u32::from(screen_entry.tile_num()) << 6) + (if bool::from(screen_entry.v_flip()) { 7 - (y_offset as u32 & 7) } else { y_offset as u32 & 7 } << 3)
                } else {
                    (u32::from(screen_entry.tile_num()) << 5) + (if bool::from(screen_entry.v_flip()) { 7 - (y_offset as u32 & 7) } else { y_offset as u32 & 7 } << 2)
                };
            let mut indices = if BIT8 {
                Self::read_bg::<u32>(char_index_addr, mem) as u64 | ((Self::read_bg::<u32>(char_index_addr + 4, mem) as u64) << 32)
            } else {
                Self::read_bg::<u32>(char_index_addr, mem) as u64
            };

            let x_origin = i.wrapping_sub(x_offset & 7);
            let mut x = x_origin;
            while indices != 0 {
                let tmp_x = if bool::from(screen_entry.h_flip()) { x_origin + 7 - x + x_origin } else { x };

                if tmp_x < 256 && (indices & if BIT8 { 0xFF } else { 0xF }) != 0 && self.is_within_window::<BG>(line, tmp_x as u8) {
                    let color = read_palettes(mem, palette_addr + ((indices as u32 & if BIT8 { 0xFF } else { 0xF }) << 1));
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
}

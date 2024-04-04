use crate::hle::gpu::gpu::DISPLAY_WIDTH;
use crate::hle::gpu::gpu_2d::{Gpu2D, Gpu2DEngine};
use crate::hle::memory::mem::Memory;

impl<const ENGINE: Gpu2DEngine> Gpu2D<ENGINE> {
    pub(super) fn draw_text<const BG: usize>(&mut self, line: u8, mem: &Memory) {
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

    pub(super) fn draw_text_pixels<const BG: usize, const BIT8: bool>(
        &mut self,
        line: u8,
        mem: &Memory,
    ) {
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
                // todo!()
            }

            let tile = self.read_bg::<u16>(tile_addr, mem);
            let tile = crate::hle::gpu::gpu_2d::TextBgScreen::from(tile);

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
}

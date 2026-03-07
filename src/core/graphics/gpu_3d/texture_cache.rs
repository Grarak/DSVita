use crate::core::graphics::gpu_3d::registers_3d::TextureFormat;
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DDraw;
use crate::core::graphics::gpu_mem_buf::{GpuMemBuf, GpuMemRefs};
use crate::core::memory::vram;
use crate::core::memory::vram::Vram;
use crate::utils;
use crate::utils::HeapDynamic;
use bilge::prelude::*;
use gl::types::GLuint;
use std::arch::arm::{
    uint16x8_t, uint16x8x2_t, uint32x4x2_t, uint32x4x4_t, uint8x16x2_t, uint8x8x2_t, vaddl_u8, vaddq_u16, vand_u8, vcombine_u32, vdup_n_u8, vdupq_n_u8, vget_high_u16, vget_high_u8, vget_low_u16,
    vget_low_u32, vget_low_u8, vld1_u16, vld1_u16_x4, vld1_u8_x2, vld1_u8_x4, vld2_u8, vld2q_u8, vmovl_u8, vmull_u8, vorr_u8, vorrq_u8, vrev64_u32, vset_lane_u32, vset_lane_u8, vsetq_lane_u32,
    vsetq_lane_u8, vshr_n_u8, vshrn_n_u16, vst1q_u32, vst1q_u32_x2, vst1q_u32_x4, vtbl1_u8, vtbl2_u8, vtbl4_u8, vzip_u8, vzipq_u16,
};
use std::cmp::min;
use std::collections::HashMap;
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::mem;
use std::mem::MaybeUninit;
use std::time::Instant;

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
struct Texture3DMetadata {
    s_shift: u3,
    t_shift: u3,
    format: TextureFormat,
    color_0_transparent: bool,
    unused: u6,
}

impl Texture3DMetadata {
    fn width(self) -> u32 {
        8 << u8::from(self.s_shift())
    }

    fn height(self) -> u32 {
        8 << u8::from(self.t_shift())
    }

    fn size(self) -> u32 {
        1 << (u8::from(self.s_shift()) + u8::from(self.t_shift()) + 6)
    }
}

pub struct Texture3D {
    vram_addr: u16,
    pal_addr: u16,
    metadata: Texture3DMetadata,
    last_used: Instant,
    tex_rear_plane_img_banks: [u8; 4],
    tex_palette_banks: [u8; 6],
    data: HeapDynamic<u32>,
    in_use: bool,
    dirty: bool,
    pub texture_id: GLuint,
}

impl Texture3D {
    unsafe fn decode_4x4(&mut self, mem_refs: &GpuMemRefs) {
        let vram_addr = (self.vram_addr as u32) << 3;
        let pal_addr = (self.pal_addr as u32) << 4;
        let mut slot1_addr = 0x20000 + ((vram_addr & 0x1FFFF) >> 1);
        if vram_addr >> 17 == 2 {
            slot1_addr += 0x10000;
        }

        let width_shift = u8::from(self.metadata.s_shift()) + 3;
        let bytes_height = self.metadata.height() >> 2;
        let bytes_width = self.metadata.width() >> 2;

        let tbl_5_to_8 = utils::vld_5_to_8_tbl();

        for by in 0..bytes_height {
            let tile_addr_base = by << (width_shift - 2);
            for bx in 0..bytes_width {
                let pal_data_addr = slot1_addr + ((tile_addr_base + bx) << 1);
                let pal_data = utils::read_from_mem::<u16>(mem_refs.tex_rear_plane_image.as_ref(), pal_data_addr);
                let pal_offset = pal_addr + (((pal_data as u32) & 0x3FFF) << 2);
                let pal_mode = pal_data >> 14;

                // let mut packed_colors: [u16; 4] = MaybeUninit::uninit().assume_init();
                // utils::read_from_mem_slice(mem_refs.tex_pal.as_ref(), pal_offset, &mut packed_colors);
                //
                // let mut colors: [u32; 4] = MaybeUninit::uninit().assume_init();
                // for i in 0..4 {
                //     colors[i] = utils::rgba5_to_rgba8(packed_colors[i] | (1 << 15));
                // }
                //
                // match pal_mode {
                //     0 => colors[3] = 0,
                //     1 => {
                //         colors[3] = 0;
                //         let r0 = colors[0] & 0xFF;
                //         let g0 = (colors[0] >> 8) & 0xFF;
                //         let b0 = (colors[0] >> 16) & 0xFF;
                //         let r1 = colors[1] & 0xFF;
                //         let g1 = (colors[1] >> 8) & 0xFF;
                //         let b1 = (colors[1] >> 16) & 0xFF;
                //         let r = (r0 + r1) >> 1;
                //         let g = (g0 + g1) >> 1;
                //         let b = (b0 + b1) >> 1;
                //         colors[2] = (0xFF << 24) | (b << 16) | (g << 8) | r;
                //     }
                //     2 => {}
                //     3 => {
                //         let r0 = colors[0] & 0xFF;
                //         let g0 = (colors[0] >> 8) & 0xFF;
                //         let b0 = (colors[0] >> 16) & 0xFF;
                //         let r1 = colors[1] & 0xFF;
                //         let g1 = (colors[1] >> 8) & 0xFF;
                //         let b1 = (colors[1] >> 16) & 0xFF;
                //         let r = (r0 * 5 + r1 * 3) >> 3;
                //         let g = (g0 * 5 + g1 * 3) >> 3;
                //         let b = (b0 * 5 + b1 * 3) >> 3;
                //         colors[2] = (0xFF << 24) | (b << 16) | (g << 8) | r;
                //
                //         let r = (r0 * 3 + r1 * 5) >> 3;
                //         let g = (g0 * 3 + g1 * 5) >> 3;
                //         let b = (b0 * 3 + b1 * 5) >> 3;
                //         colors[3] = (0xFF << 24) | (b << 16) | (g << 8) | r;
                //     }
                //     _ => unreachable_unchecked(),
                // }

                let packed_colors = vld1_u16(mem_refs.tex_pal.as_ptr().add(pal_offset as usize) as _);
                let mut unpacked_colors = utils::vunpack_rgb5_to_rgb8::<false>(packed_colors, tbl_5_to_8);

                match pal_mode {
                    0 => unpacked_colors = vsetq_lane_u32::<3>(0, unpacked_colors),
                    1 => {
                        let colors01 = vget_low_u32(unpacked_colors);
                        let colors10 = vrev64_u32(colors01);
                        let colors_sum = vaddl_u8(mem::transmute(colors01), mem::transmute(colors10));
                        let color2 = vshrn_n_u16::<1>(colors_sum);
                        unpacked_colors = vcombine_u32(colors01, vset_lane_u32::<1>(0, mem::transmute(color2)));
                    }
                    2 => {}
                    3 => {
                        let colors01 = vget_low_u32(unpacked_colors);
                        let colors10 = vrev64_u32(colors01);
                        let colors01l = vmull_u8(mem::transmute(colors01), vdup_n_u8(5));
                        let colors10l = vmull_u8(mem::transmute(colors10), vdup_n_u8(3));
                        let colors23 = vaddq_u16(colors01l, colors10l);
                        let colors23 = vshrn_n_u16::<3>(colors23);
                        unpacked_colors = vcombine_u32(colors01, mem::transmute(colors23));
                    }
                    _ => unreachable_unchecked(),
                }

                let mut colors: [u32; 4] = MaybeUninit::uninit().assume_init();
                vst1q_u32(colors.as_mut_ptr(), unpacked_colors);

                let texel_addr = vram_addr + ((tile_addr_base + bx) << 2);
                let texels = utils::read_from_mem::<u32>(mem_refs.tex_rear_plane_image.as_ref(), texel_addr);

                for y_offset in 0..4 {
                    let y_texels = texels >> (y_offset << 3);
                    for x_offset in 0..4 {
                        let texel = (y_texels >> (x_offset << 1)) & 0x3;
                        let index = (((by << 2) + y_offset) << (width_shift)) + (bx << 2) + x_offset;
                        *self.data.get_unchecked_mut(index as usize) = colors[texel as usize];
                    }
                }
            }
        }
    }

    unsafe fn decode_pal4(&mut self, mem_refs: &GpuMemRefs) {
        let vram_addr = (self.vram_addr as u32) << 3;
        let pal_addr = (self.pal_addr as u32) << 3;

        let mut pal_tbl = vld2_u8(mem_refs.tex_pal.as_ptr().add(pal_addr as usize) as _);
        pal_tbl.1 = vorr_u8(pal_tbl.1, vdup_n_u8(1 << 7));
        if self.metadata.color_0_transparent() {
            pal_tbl.0 = vset_lane_u8::<0>(0, pal_tbl.0);
            pal_tbl.1 = vset_lane_u8::<0>(0, pal_tbl.1);
        }

        let tbl_5_to_8 = utils::vld_5_to_8_tbl();

        for i in (0..self.metadata.size()).step_by(64) {
            let texels = vld1_u8_x2(mem_refs.tex_rear_plane_image.as_ptr().add((vram_addr + (i >> 2)) as usize));
            let texel_mask = vdup_n_u8(0x3);
            let texels_0 = [vand_u8(texels.0, texel_mask), vand_u8(texels.1, texel_mask)];
            let texels_1 = [vand_u8(vshr_n_u8::<2>(texels.0), texel_mask), vand_u8(vshr_n_u8::<2>(texels.1), texel_mask)];
            let texels_2 = [vand_u8(vshr_n_u8::<4>(texels.0), texel_mask), vand_u8(vshr_n_u8::<4>(texels.1), texel_mask)];
            let texels_3 = [vshr_n_u8::<6>(texels.0), vshr_n_u8::<6>(texels.1)];

            let mut texels_01: [uint8x8x2_t; 2] = MaybeUninit::uninit().assume_init();
            let mut texels_23: [uint8x8x2_t; 2] = MaybeUninit::uninit().assume_init();
            for j in 0..2 {
                texels_01[j] = vzip_u8(texels_0[j], texels_1[j]);
                texels_23[j] = vzip_u8(texels_2[j], texels_3[j]);
            }

            let texels_low = vzipq_u16(mem::transmute(texels_01[0]), mem::transmute(texels_23[0]));
            let texels_high = vzipq_u16(mem::transmute(texels_01[1]), mem::transmute(texels_23[1]));
            let texels: [uint8x8x2_t; 4] = [mem::transmute(texels_low.0), mem::transmute(texels_low.1), mem::transmute(texels_high.0), mem::transmute(texels_high.1)];

            let mut pixels: [uint16x8x2_t; 4] = MaybeUninit::uninit().assume_init();
            for j in 0..4 {
                let texels_low = vtbl1_u8(pal_tbl.0, vget_low_u8(mem::transmute(texels[j])));
                let texels_high = vtbl1_u8(pal_tbl.1, vget_low_u8(mem::transmute(texels[j])));
                let pixels_low = vzip_u8(texels_low, texels_high);

                let texels_low = vtbl1_u8(pal_tbl.0, vget_high_u8(mem::transmute(texels[j])));
                let texels_high = vtbl1_u8(pal_tbl.1, vget_high_u8(mem::transmute(texels[j])));
                let pixels_high = vzip_u8(texels_low, texels_high);

                pixels[j] = uint16x8x2_t(mem::transmute(pixels_low), mem::transmute(pixels_high));
            }

            for j in 0..4 {
                let low_low = utils::vunpack_rgb5_to_rgb8::<true>(vget_low_u16(pixels[j].0), tbl_5_to_8);
                let low_high = utils::vunpack_rgb5_to_rgb8::<true>(vget_high_u16(pixels[j].0), tbl_5_to_8);
                let high_low = utils::vunpack_rgb5_to_rgb8::<true>(vget_low_u16(pixels[j].1), tbl_5_to_8);
                let high_high = utils::vunpack_rgb5_to_rgb8::<true>(vget_high_u16(pixels[j].1), tbl_5_to_8);
                let pixels = uint32x4x4_t(low_low, low_high, high_low, high_high);
                vst1q_u32_x4(self.data.as_mut_ptr().add(i as usize + (j << 4)), pixels);
            }
        }
    }

    unsafe fn decode_pal16(&mut self, mem_refs: &GpuMemRefs) {
        let vram_addr = (self.vram_addr as u32) << 3;
        let pal_addr = (self.pal_addr as u32) << 4;

        let mut pal_tbl = vld2q_u8(mem_refs.tex_pal.as_ptr().add(pal_addr as usize));
        pal_tbl.1 = vorrq_u8(pal_tbl.1, vdupq_n_u8(1 << 7));
        if self.metadata.color_0_transparent() {
            pal_tbl.0 = vsetq_lane_u8::<0>(0, pal_tbl.0);
            pal_tbl.1 = vsetq_lane_u8::<0>(0, pal_tbl.1);
        }

        let tbl_5_to_8 = utils::vld_5_to_8_tbl();

        for i in (0..self.metadata.size()).step_by(64) {
            let texels = vld1_u8_x4(mem_refs.tex_rear_plane_image.as_ptr().add((vram_addr + (i >> 1)) as usize));
            let texel_mask = vdup_n_u8(0xF);
            let texels_even = [
                vand_u8(texels.0, texel_mask),
                vand_u8(texels.1, texel_mask),
                vand_u8(texels.2, texel_mask),
                vand_u8(texels.3, texel_mask),
            ];
            let texels_odd = [vshr_n_u8::<4>(texels.0), vshr_n_u8::<4>(texels.1), vshr_n_u8::<4>(texels.2), vshr_n_u8::<4>(texels.3)];
            let mut texels: [uint8x8x2_t; 4] = MaybeUninit::uninit().assume_init();
            for j in 0..4 {
                texels[j] = vzip_u8(texels_even[j], texels_odd[j]);
            }

            let mut pixels: [uint16x8x2_t; 4] = MaybeUninit::uninit().assume_init();
            for j in 0..4 {
                let texels_low = vtbl2_u8(mem::transmute(pal_tbl.0), vget_low_u8(mem::transmute(texels[j])));
                let texels_high = vtbl2_u8(mem::transmute(pal_tbl.1), vget_low_u8(mem::transmute(texels[j])));
                let pixels_low = vzip_u8(texels_low, texels_high);

                let texels_low = vtbl2_u8(mem::transmute(pal_tbl.0), vget_high_u8(mem::transmute(texels[j])));
                let texels_high = vtbl2_u8(mem::transmute(pal_tbl.1), vget_high_u8(mem::transmute(texels[j])));
                let pixels_high = vzip_u8(texels_low, texels_high);

                pixels[j] = uint16x8x2_t(mem::transmute(pixels_low), mem::transmute(pixels_high));
            }

            for j in 0..4 {
                let low_low = utils::vunpack_rgb5_to_rgb8::<true>(vget_low_u16(pixels[j].0), tbl_5_to_8);
                let low_high = utils::vunpack_rgb5_to_rgb8::<true>(vget_high_u16(pixels[j].0), tbl_5_to_8);
                let high_low = utils::vunpack_rgb5_to_rgb8::<true>(vget_low_u16(pixels[j].1), tbl_5_to_8);
                let high_high = utils::vunpack_rgb5_to_rgb8::<true>(vget_high_u16(pixels[j].1), tbl_5_to_8);
                let pixels = uint32x4x4_t(low_low, low_high, high_low, high_high);
                vst1q_u32_x4(self.data.as_mut_ptr().add(i as usize + (j << 4)), pixels);
            }
        }

        // for i in (0..self.metadata.size()).step_by(64) {
        //     let mut texels: [u8; 32] = MaybeUninit::uninit().assume_init();
        //     utils::read_from_mem_slice(mem_refs.tex_rear_plane_image.as_ref(), vram_addr + (i >> 1), &mut texels);
        //     for j in 0..32 {
        //         for k in 0..2 {
        //             let texel = (texels[j] >> (k << 2)) & 0xF;
        //             let color = utils::read_from_mem::<u16>(mem_refs.tex_pal.as_ref(), pal_addr + ((texel as u32) << 1));
        //             let color = if texel == 0 { (0xFF << 24) | (0xFF) } else { utils::rgb5_to_rgb8(color) };
        //             self.data[(i + (j << 1) as u32 + k) as usize] = color;
        //         }
        //     }
        // }
    }

    unsafe fn decode_pal256(&mut self, mem_refs: &GpuMemRefs) {
        let vram_addr = (self.vram_addr as u32) << 3;
        let pal_addr = (self.pal_addr as u32) << 4;

        let tbl_5_to_8 = utils::vld_5_to_8_tbl();

        for i in (0..self.metadata.size()).step_by(16) {
            let mut colors: [u16; 16] = MaybeUninit::uninit().assume_init();

            for j in 0..16 {
                let pal_index = utils::read_from_mem::<u8>(mem_refs.tex_rear_plane_image.as_ref(), vram_addr + i + j as u32) as u32;
                if self.metadata.color_0_transparent() && pal_index == 0 {
                    colors[j] = 0;
                } else {
                    colors[j] = utils::read_from_mem::<u16>(mem_refs.tex_pal.as_ref(), pal_addr + (pal_index << 1)) | (1 << 15);
                }
            }

            let packed_colors = vld1_u16_x4(colors.as_ptr());
            let packed_colors = [packed_colors.0, packed_colors.1, packed_colors.2, packed_colors.3];
            for j in 0..4 {
                let colors = utils::vunpack_rgb5_to_rgb8::<true>(packed_colors[j], tbl_5_to_8);
                vst1q_u32(self.data.as_mut_ptr().add(i as usize + (j << 2)), colors);
            }
        }
    }

    unsafe fn decode_a5i3(&mut self, mem_refs: &GpuMemRefs) {
        let vram_addr = (self.vram_addr as u32) << 3;
        let pal_addr = (self.pal_addr as u32) << 4;

        let pal_tbl = vld2_u8(mem_refs.tex_pal.as_ptr().add(pal_addr as usize));

        let tbl_5_to_8 = utils::vld_5_to_8_tbl();

        for i in (0..self.metadata.size()).step_by(32) {
            let texels = vld1_u8_x4(mem_refs.tex_rear_plane_image.as_ptr().add((vram_addr + i) as usize));
            let pal_mask = vdup_n_u8(0x7);
            let texels_pal = [vand_u8(texels.0, pal_mask), vand_u8(texels.1, pal_mask), vand_u8(texels.2, pal_mask), vand_u8(texels.3, pal_mask)];
            let texels_alpha = [vshr_n_u8::<3>(texels.0), vshr_n_u8::<3>(texels.1), vshr_n_u8::<3>(texels.2), vshr_n_u8::<3>(texels.3)];

            let mut pixels: [uint16x8_t; 4] = MaybeUninit::uninit().assume_init();
            for j in 0..4 {
                let pixels_low = vtbl1_u8(pal_tbl.0, texels_pal[j]);
                let pixels_high = vtbl1_u8(pal_tbl.1, texels_pal[j]);
                pixels[j] = mem::transmute(vzip_u8(pixels_low, pixels_high));
            }

            for j in 0..4 {
                let alpha = vmovl_u8(texels_alpha[j]);
                let low = utils::vunpack_rgba5_to_rgba8(vget_low_u16(pixels[j]), vget_low_u16(alpha), tbl_5_to_8);
                let high = utils::vunpack_rgba5_to_rgba8(vget_high_u16(pixels[j]), vget_high_u16(alpha), tbl_5_to_8);
                let pixels = uint32x4x2_t(low, high);
                vst1q_u32_x2(self.data.as_mut_ptr().add(i as usize + (j << 3)), pixels);
            }
        }
    }

    unsafe fn decode_a3i5(&mut self, mem_refs: &GpuMemRefs) {
        let vram_addr = (self.vram_addr as u32) << 3;
        let pal_addr = (self.pal_addr as u32) << 4;

        let pal_tbl_0 = vld2q_u8(mem_refs.tex_pal.as_ptr().add(pal_addr as usize));
        let pal_tbl_1 = vld2q_u8(mem_refs.tex_pal.as_ptr().add(pal_addr as usize + 32));
        let pal_tbl_low = uint8x16x2_t(pal_tbl_0.0, pal_tbl_1.0);
        let pal_tbl_high = uint8x16x2_t(pal_tbl_0.1, pal_tbl_1.1);

        let tbl_5_to_8 = utils::vld_5_to_8_tbl();
        let tbl_3_to_5 = utils::vld_3_to_5_tbl();

        for i in (0..self.metadata.size()).step_by(32) {
            let texels = vld1_u8_x4(mem_refs.tex_rear_plane_image.as_ptr().add((vram_addr + i) as usize));
            let pal_mask = vdup_n_u8(0x1F);
            let texels_pal = [vand_u8(texels.0, pal_mask), vand_u8(texels.1, pal_mask), vand_u8(texels.2, pal_mask), vand_u8(texels.3, pal_mask)];
            let mut texels_alpha = [vshr_n_u8::<5>(texels.0), vshr_n_u8::<5>(texels.1), vshr_n_u8::<5>(texels.2), vshr_n_u8::<5>(texels.3)];
            for j in 0..4 {
                texels_alpha[j] = vtbl1_u8(tbl_3_to_5, texels_alpha[j]);
            }

            let mut pixels: [uint16x8_t; 4] = MaybeUninit::uninit().assume_init();
            for j in 0..4 {
                let pixels_low = vtbl4_u8(mem::transmute(pal_tbl_low), texels_pal[j]);
                let pixels_high = vtbl4_u8(mem::transmute(pal_tbl_high), texels_pal[j]);
                pixels[j] = mem::transmute(vzip_u8(pixels_low, pixels_high));
            }

            for j in 0..4 {
                let alpha = vmovl_u8(texels_alpha[j]);
                let low = utils::vunpack_rgba5_to_rgba8(vget_low_u16(pixels[j]), vget_low_u16(alpha), tbl_5_to_8);
                let high = utils::vunpack_rgba5_to_rgba8(vget_high_u16(pixels[j]), vget_high_u16(alpha), tbl_5_to_8);
                let pixels = uint32x4x2_t(low, high);
                vst1q_u32_x2(self.data.as_mut_ptr().add(i as usize + (j << 3)), pixels);
            }
        }
    }

    unsafe fn decode_direct(&mut self, mem_refs: &GpuMemRefs) {
        let vram_addr = (self.vram_addr as u32) << 3;

        let tbl_5_to_8 = utils::vld_5_to_8_tbl();

        for i in (0..self.metadata.size()).step_by(16) {
            let colors = vld1_u16_x4(mem_refs.tex_rear_plane_image.as_ptr().add((vram_addr + (i << 1)) as usize) as _);
            let colors = [colors.0, colors.1, colors.2, colors.3];

            for j in 0..4 {
                let colors = utils::vunpack_rgb5_to_rgb8::<true>(colors[j], tbl_5_to_8);
                vst1q_u32(self.data.as_mut_ptr().add(i as usize + (j << 2)), colors);
            }
        }
    }

    fn new(draw: &Gpu3DDraw, vram: &Vram, mem_refs: &GpuMemRefs) -> Self {
        let metadata = Texture3DMetadata::new(
            draw.tex_image_param.size_s_shift(),
            draw.tex_image_param.size_t_shift(),
            draw.tex_image_param.format(),
            draw.tex_image_param.color_0_transparent(),
            u6::new(0),
        );

        let mut instance = Texture3D {
            vram_addr: draw.tex_image_param.vram_offset(),
            pal_addr: draw.pal_addr,
            metadata,
            last_used: Instant::now(),
            tex_rear_plane_img_banks: vram.maps.tex_rear_plane_img_banks,
            tex_palette_banks: vram.maps.tex_palette_banks,
            data: unsafe { HeapDynamic::uninitialized(metadata.size() as usize) },
            in_use: true,
            dirty: false,
            texture_id: u32::MAX,
        };
        unsafe { instance.decode_texture(mem_refs) };
        instance
    }

    pub unsafe fn get_texture_id(&mut self) -> GLuint {
        if self.texture_id == u32::MAX {
            let mut tex = 0;
            gl::GenTextures(1, &mut tex);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as _,
                self.metadata.width() as _,
                self.metadata.height() as _,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                self.data.as_ptr() as _,
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
            gl::BindTexture(gl::TEXTURE_2D, 0);

            self.texture_id = tex;
            self.data.destroy();
        }
        self.texture_id
    }

    unsafe fn decode_texture(&mut self, mem_refs: &GpuMemRefs) {
        const DECODE_FUNS: [unsafe fn(&mut Texture3D, mem_refs: &GpuMemRefs); 7] = [
            Texture3D::decode_a3i5,
            Texture3D::decode_pal4,
            Texture3D::decode_pal16,
            Texture3D::decode_pal256,
            Texture3D::decode_4x4,
            Texture3D::decode_a5i3,
            Texture3D::decode_direct,
        ];

        DECODE_FUNS[self.metadata.format() as usize - 1](self, mem_refs);

        // let file = File::create(format!(
        //     "texture_{}_{}_{:?}_{:?}_{:x}_{:x}.bmp",
        //     self.metadata.width(),
        //     self.metadata.height(),
        //     SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_micros(),
        //     self.metadata.format(),
        //     (self.vram_addr as u32) << 3,
        //     self.pal_addr as u32,
        // ))
        // .unwrap();
        //
        // let mut header = [
        //     0x42, 0x4D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x36, 0x00, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x20, 0x00, 0x00,
        //     0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13, 0x0B, 0x00, 0x00, 0x13, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // ];
        // utils::write_to_mem::<u32>(&mut header, 0x2, 54 + self.data.len() as u32);
        // utils::write_to_mem::<u32>(&mut header, 0x12, self.metadata.width() as _);
        // utils::write_to_mem::<u32>(&mut header, 0x16, self.metadata.height() as _);
        // utils::write_to_mem::<u32>(&mut header, 0x22, self.data.len() as u32);
        // file.write_all_at(&header, 0).unwrap();
        // file.write_all_at(slice::from_raw_parts(self.data.as_ptr() as _, self.data.len() * 4), header.len() as u64).unwrap();
    }

    fn is_dirty(&self, mem_buf: &GpuMemBuf) -> bool {
        const TEX_SIZE_SHIFTS: [u8; 7] = [2, 0, 1, 2, 0, 2, 3];

        let tex_size_bytes = (self.metadata.size() << TEX_SIZE_SHIFTS[self.metadata.format() as usize - 1]) >> 2;
        let vram_addr = (self.vram_addr as u32) << 3;
        let vram_addr_end = vram_addr + tex_size_bytes;

        if mem_buf
            .vram
            .maps
            .is_tex_rear_plane_img_dirty(vram_addr, vram_addr_end, &self.tex_rear_plane_img_banks, &mem_buf.vram_banks.dirty_sections)
        {
            return true;
        }

        if self.metadata.format() == TextureFormat::Texel4x4Compressed {
            let mut slot1_addr = 0x20000 + ((vram_addr & 0x1FFFF) >> 1);
            if vram_addr >> 17 == 2 {
                slot1_addr += 0x10000;
            }
            let slot1_addr_end = slot1_addr + (self.metadata.size() >> 3);
            if mem_buf
                .vram
                .maps
                .is_tex_rear_plane_img_dirty(slot1_addr, slot1_addr_end, &self.tex_rear_plane_img_banks, &mem_buf.vram_banks.dirty_sections)
            {
                return true;
            }
        }

        if self.metadata.format() != TextureFormat::Direct {
            const PAL_SIZE_SHIFTS: [u8; 6] = [5, 2, 4, 8, 15, 3];
            let pal_size_bytes = 1 << (PAL_SIZE_SHIFTS[self.metadata.format() as usize - 1] + 1);
            let pal_addr = (self.pal_addr as u32) << if self.metadata.format() == TextureFormat::Color4Palette { 3 } else { 4 };
            let pal_addr_end = pal_addr + pal_size_bytes;
            let pal_addr_end = min(vram::TEX_PAL_SIZE as u32, pal_addr_end);
            if mem_buf
                .vram
                .maps
                .is_tex_palette_dirty(pal_addr, pal_addr_end, &self.tex_palette_banks, &mem_buf.vram_banks.dirty_sections)
            {
                return true;
            }
        }

        false
    }
}

const CACHE_SIZE_LIMIT: u32 = 16 * 1024 * 1024;

pub struct Texture3DCache {
    cache: HashMap<u64, Box<Texture3D>, utils::BuildNoHasher64>,
    total_size: u32,
}

impl Texture3DCache {
    pub fn new() -> Self {
        Texture3DCache {
            cache: HashMap::default(),
            total_size: 0,
        }
    }

    pub fn clear(&mut self) {
        for texture_3d in self.cache.values() {
            if texture_3d.texture_id != u32::MAX {
                unsafe { gl::DeleteTextures(1, &texture_3d.texture_id) };
            }
        }
        self.cache.clear();
    }

    pub fn mark_dirty(&mut self, mem_buf: &GpuMemBuf) {
        for texture_3d in self.cache.values_mut() {
            if !texture_3d.dirty && texture_3d.is_dirty(mem_buf) {
                texture_3d.dirty = true;
            }
        }
    }

    pub fn get(&mut self, draw: &Gpu3DDraw, mem_buf: &GpuMemBuf, mem_refs: &GpuMemRefs, texture_ids_to_delete: &mut Vec<GLuint>) -> &mut Texture3D {
        let key = draw.key();
        if let Some(texture_3d) = self.cache.get_mut(&key) {
            if texture_3d.in_use || !texture_3d.dirty {
                texture_3d.last_used = Instant::now();
                texture_3d.in_use = true;
                return unsafe { mem::transmute(texture_3d.as_mut()) };
            } else {
                self.total_size -= texture_3d.metadata.size();
                if texture_3d.texture_id != u32::MAX {
                    texture_ids_to_delete.push(texture_3d.texture_id);
                }
                self.cache.remove(&key);
            }
        }

        let texture_3d = Texture3D::new(draw, &mem_buf.vram, mem_refs);
        while self.total_size + texture_3d.metadata.size() >= CACHE_SIZE_LIMIT {
            let mut oldest_key = 0;
            let mut oldest_timestamp = Instant::now();
            let mut oldest_size = 0;
            unsafe { assert_unchecked(!self.cache.is_empty()) };
            for (&key, texture_3d) in &self.cache {
                if texture_3d.dirty || (!texture_3d.in_use && texture_3d.last_used < oldest_timestamp) {
                    oldest_key = key;
                    oldest_timestamp = texture_3d.last_used;
                    oldest_size = texture_3d.metadata.size();
                }
            }
            assert_ne!(oldest_size, 0);
            self.total_size -= oldest_size;
            unsafe { self.cache.remove(&oldest_key).unwrap_unchecked() };
        }
        self.total_size += texture_3d.metadata.size();
        self.cache.insert(key, Box::new(texture_3d));
        unsafe { self.cache.get_mut(&key).unwrap_unchecked().as_mut() }
    }

    pub fn reset_usage(&mut self) {
        for texture_3d in self.cache.values_mut() {
            texture_3d.in_use = false;
        }
    }
}

use crate::core::memory::mem::Memory;
use crate::core::memory::{regions, vram};
use crate::utils::HeapMemU8;

#[derive(Default)]
pub struct GpuMemBuf {
    pub lcdc: HeapMemU8<{ vram::TOTAL_SIZE }>,

    pub bg_a: HeapMemU8<{ vram::BG_A_SIZE as usize }>,
    pub obj_a: HeapMemU8<{ vram::OBJ_A_SIZE as usize }>,
    pub bg_a_ext_palette: HeapMemU8<{ vram::BG_EXT_PAL_SIZE as usize }>,
    pub obj_a_ext_palette: HeapMemU8<{ vram::OBJ_EXT_PAL_SIZE as usize }>,

    pub bg_b: HeapMemU8<{ vram::BG_B_SIZE as usize }>,
    pub obj_b: HeapMemU8<{ vram::OBJ_B_SIZE as usize }>,
    pub bg_b_ext_palette: HeapMemU8<{ vram::BG_EXT_PAL_SIZE as usize }>,
    pub obj_b_ext_palette: HeapMemU8<{ vram::OBJ_EXT_PAL_SIZE as usize }>,

    pub pal_a: HeapMemU8<{ regions::STANDARD_PALETTES_SIZE as usize / 2 }>,
    pub pal_b: HeapMemU8<{ regions::STANDARD_PALETTES_SIZE as usize / 2 }>,
    pub oam_a: HeapMemU8<{ regions::OAM_SIZE as usize / 2 }>,
    pub oam_b: HeapMemU8<{ regions::OAM_SIZE as usize / 2 }>,

    pub tex_rear_plane_image: HeapMemU8<{ vram::TEX_REAR_PLANE_IMAGE_SIZE as usize }>,
    pub tex_pal: HeapMemU8<{ vram::TEX_PAL_SIZE as usize }>,
}

impl GpuMemBuf {
    pub fn read_2d(&mut self, mem: &mut Memory, read_lcdc: bool) {
        if read_lcdc {
            mem.vram.read_all_lcdc(&mut self.lcdc);
        }

        mem.vram.read_all_bg_a(&mut self.bg_a);
        mem.vram.read_all_obj_a(&mut self.obj_a);
        mem.vram.read_all_bg_a_ext_palette(&mut self.bg_a_ext_palette);
        mem.vram.read_all_obj_a_ext_palette(&mut self.obj_a_ext_palette);

        mem.vram.read_bg_b(&mut self.bg_b);
        mem.vram.read_all_obj_b(&mut self.obj_b);
        mem.vram.read_all_bg_b_ext_palette(&mut self.bg_b_ext_palette);
        mem.vram.read_all_obj_b_ext_palette(&mut self.obj_b_ext_palette);

        if mem.palettes.dirty {
            mem.palettes.dirty = false;
            self.pal_a.copy_from_slice(&mem.palettes.mem[..mem.palettes.mem.len() / 2]);
            self.pal_b.copy_from_slice(&mem.palettes.mem[mem.palettes.mem.len() / 2..]);
        }

        if mem.oam.dirty {
            mem.oam.dirty = false;
            self.oam_a.copy_from_slice(&mem.oam.mem[..mem.oam.mem.len() / 2]);
            self.oam_b.copy_from_slice(&mem.oam.mem[mem.oam.mem.len() / 2..]);
        }
    }

    pub fn read_3d(&mut self, mem: &mut Memory) {
        mem.vram.read_all_tex_rear_plane_img(&mut self.tex_rear_plane_image);
        mem.vram.read_all_tex_palette(&mut self.tex_pal);
    }
}

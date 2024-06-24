use crate::emu::gpu::gpu_2d::Gpu2DEngine::{A, B};
use crate::emu::memory::mem::Memory;
use crate::emu::memory::{regions, vram};
use crate::utils::HeapMemU8;

#[derive(Default)]
pub struct Gpu2dMemBuf {
    pub bg_a: HeapMemU8<{ vram::BG_A_SIZE as usize }>,
    pub obj_a: HeapMemU8<{ vram::OBJ_A_SIZE as usize }>,
    pub bg_a_ext_palette: HeapMemU8<{ vram::BG_EXT_PAL_SIZE as usize }>,
    pub obj_a_ext_palette: HeapMemU8<{ vram::OBJ_EXT_PAL_SIZE as usize }>,
    pub bg_a_ext_palette_mapped: [bool; 4],
    pub obj_a_ext_palette_mapped: bool,

    pub bg_b: HeapMemU8<{ vram::BG_B_SIZE as usize }>,
    pub obj_b: HeapMemU8<{ vram::OBJ_B_SIZE as usize }>,
    pub bg_b_ext_palette: HeapMemU8<{ vram::BG_EXT_PAL_SIZE as usize }>,
    pub obj_b_ext_palette: HeapMemU8<{ vram::OBJ_EXT_PAL_SIZE as usize }>,
    pub bg_b_ext_palette_mapped: [bool; 4],
    pub obj_b_ext_palette_mapped: bool,

    pub pal_a: HeapMemU8<{ regions::STANDARD_PALETTES_SIZE as usize / 2 }>,
    pub pal_b: HeapMemU8<{ regions::STANDARD_PALETTES_SIZE as usize / 2 }>,
    pub oam_a: HeapMemU8<{ regions::OAM_SIZE as usize / 2 }>,
    pub oam_b: HeapMemU8<{ regions::OAM_SIZE as usize / 2 }>,
}

impl Gpu2dMemBuf {
    pub fn read(&mut self, mem: &mut Memory) {
        mem.vram.read_all_bg_a(&mut self.bg_a);
        mem.vram.read_all_obj_a(&mut self.obj_a);
        mem.vram.read_all_bg_a_ext_palette(&mut self.bg_a_ext_palette);
        mem.vram.read_all_obj_a_ext_palette(&mut self.obj_a_ext_palette);
        for slot in 0..4 {
            self.bg_a_ext_palette_mapped[slot] = mem.vram.is_bg_ext_palette_mapped::<{ A }>(slot);
        }
        self.obj_a_ext_palette_mapped = mem.vram.is_obj_ext_palette_mapped::<{ A }>();

        mem.vram.read_bg_b(&mut self.bg_b);
        mem.vram.read_all_obj_b(&mut self.obj_b);
        mem.vram.read_all_bg_b_ext_palette(&mut self.bg_b_ext_palette);
        mem.vram.read_all_obj_b_ext_palette(&mut self.obj_b_ext_palette);
        for slot in 0..4 {
            self.bg_b_ext_palette_mapped[slot] = mem.vram.is_bg_ext_palette_mapped::<{ B }>(slot);
        }
        self.obj_b_ext_palette_mapped = mem.vram.is_obj_ext_palette_mapped::<{ B }>();

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
}

use crate::core::graphics::gpu::DispCapCnt;
use crate::core::memory::regions::{OAM_SIZE, STANDARD_PALETTES_SIZE};
use crate::core::memory::vram::{Vram, VramBanks, VramCnt};
use crate::core::memory::{regions, vram};
use crate::utils::{self, HeapArrayU8, PtrWrapper};

pub const CAPTURE_IDENTIFIER: &[u8; 8] = b"CAPTURE_";

#[derive(Default)]
pub struct GpuMemRefs {
    pub lcdc: PtrWrapper<[u8; vram::TOTAL_SIZE]>,

    pub bg_a: PtrWrapper<[u8; vram::BG_A_SIZE]>,
    pub obj_a: PtrWrapper<[u8; vram::OBJ_A_SIZE]>,
    pub bg_a_ext_pal: PtrWrapper<[u8; vram::BG_EXT_PAL_SIZE]>,
    pub obj_a_ext_pal: PtrWrapper<[u8; vram::OBJ_EXT_PAL_SIZE]>,
    pub pal_a: PtrWrapper<[u8; regions::STANDARD_PALETTES_SIZE as usize / 2]>,
    pub oam_a: PtrWrapper<[u8; regions::OAM_SIZE as usize / 2]>,

    pub bg_b: PtrWrapper<[u8; vram::BG_B_SIZE]>,
    pub obj_b: PtrWrapper<[u8; vram::OBJ_B_SIZE]>,
    pub bg_b_ext_pal: PtrWrapper<[u8; vram::BG_EXT_PAL_SIZE]>,
    pub obj_b_ext_pal: PtrWrapper<[u8; vram::OBJ_EXT_PAL_SIZE]>,
    pub pal_b: PtrWrapper<[u8; regions::STANDARD_PALETTES_SIZE as usize / 2]>,
    pub oam_b: PtrWrapper<[u8; regions::OAM_SIZE as usize / 2]>,

    pub tex_rear_plane_image: PtrWrapper<[u8; vram::TEX_REAR_PLANE_IMAGE_SIZE]>,
    pub tex_pal: PtrWrapper<[u8; vram::TEX_PAL_SIZE]>,
}

#[derive(Default)]
pub struct GpuMemBuf {
    pub vram: Vram,
    queued_vram_cnt: [u8; vram::BANK_SIZE],
    vram_mem: HeapArrayU8<{ vram::TOTAL_SIZE }>,
    pub pal: HeapArrayU8<{ regions::STANDARD_PALETTES_SIZE as usize }>,
    pub oam: HeapArrayU8<{ regions::OAM_SIZE as usize }>,
}

impl GpuMemBuf {
    pub fn init(&mut self) {
        self.vram = Vram::default();
    }

    pub fn queue_vram(&mut self, vram: &Vram) {
        self.queued_vram_cnt = vram.cnt;
    }

    pub fn read_vram(&mut self, vram: &[u8; vram::TOTAL_SIZE]) {
        for (i, &cnt) in self.queued_vram_cnt.iter().enumerate() {
            let cnt = VramCnt::from(cnt);
            if cnt.enable() {
                VramBanks::copy_bank(i as u8, &mut self.vram_mem, vram);
            }
        }
    }

    pub fn read_palettes_oam(&mut self, palettes: &[u8; STANDARD_PALETTES_SIZE as usize], oam: &[u8; OAM_SIZE as usize]) {
        self.pal.copy_from_slice(palettes);
        self.oam.copy_from_slice(oam);
    }

    pub fn use_queued_vram(&mut self) {
        self.vram.cnt = self.queued_vram_cnt;
    }

    pub fn rebuild_vram_maps(&mut self) {
        self.vram.rebuild_maps();
    }

    pub fn read_all(&self, refs: &mut GpuMemRefs, read_lcdc: bool, read_3d: bool) {
        if read_lcdc {
            self.vram.maps.read_all_lcdc(&mut refs.lcdc, &self.vram_mem);
        }

        self.vram.maps.read_all_bg_a(&mut refs.bg_a, &self.vram_mem);
        self.vram.maps.read_all_obj_a(&mut refs.obj_a, &self.vram_mem);
        self.vram.maps.read_all_bg_a_ext_palette(&mut refs.bg_a_ext_pal, &self.vram_mem);
        self.vram.maps.read_all_obj_a_ext_palette(&mut refs.obj_a_ext_pal, &self.vram_mem);
        refs.pal_a.copy_from_slice(&self.pal[..regions::STANDARD_PALETTES_SIZE as usize / 2]);
        refs.oam_a.copy_from_slice(&self.oam[..regions::OAM_SIZE as usize / 2]);

        self.vram.maps.read_bg_b(&mut refs.bg_b, &self.vram_mem);
        self.vram.maps.read_all_obj_b(&mut refs.obj_b, &self.vram_mem);
        self.vram.maps.read_all_bg_b_ext_palette(&mut refs.bg_b_ext_pal, &self.vram_mem);
        self.vram.maps.read_all_obj_b_ext_palette(&mut refs.obj_b_ext_pal, &self.vram_mem);
        refs.pal_b.copy_from_slice(&self.pal[regions::STANDARD_PALETTES_SIZE as usize / 2..]);
        refs.oam_b.copy_from_slice(&self.oam[regions::OAM_SIZE as usize / 2..]);

        if read_3d {
            self.vram.maps.read_all_tex_rear_plane_img(&mut refs.tex_rear_plane_image, &self.vram_mem);
            self.vram.maps.read_all_tex_palette(&mut refs.tex_pal, &self.vram_mem);
        }
    }

    pub fn mark_block_as_captured(disp_cap_cnt: DispCapCnt, bank: &mut [u8; vram::BANK_A_SIZE]) {
        utils::write_to_mem_slice(bank, disp_cap_cnt.write_offset() as usize, CAPTURE_IDENTIFIER);
        utils::write_to_mem(bank, disp_cap_cnt.write_offset() + CAPTURE_IDENTIFIER.len() as u32, disp_cap_cnt);
    }

    pub fn unmark_blocks_as_captured(vram_mem: &mut [u8; vram::TOTAL_SIZE]) {
        for bank_num in 0..4 {
            for offset_num in 0..4 {
                let offset = offset_num * 0x8000;
                let bank = &mut vram_mem[bank_num * vram::BANK_A_SIZE + offset..];
                if &bank[..CAPTURE_IDENTIFIER.len()] == CAPTURE_IDENTIFIER {
                    bank[..CAPTURE_IDENTIFIER.len()].fill(0);
                }
            }
        }
    }

    pub fn insert_capture_mem(&mut self, capture_mem: &[u8; vram::BANK_A_SIZE * 4]) {
        for bank_num in 0..4 {
            if !VramCnt::from(self.vram.cnt[bank_num]).enable() {
                continue;
            }

            for offset_num in 0..4 {
                let offset = offset_num * 0x8000;
                let bank = &mut self.vram_mem[bank_num * vram::BANK_A_SIZE + offset..];
                let disp_cap_cnt = utils::read_from_mem::<DispCapCnt>(bank, CAPTURE_IDENTIFIER.len() as u32);
                if &bank[..CAPTURE_IDENTIFIER.len()] == CAPTURE_IDENTIFIER
                    && disp_cap_cnt.capture_enabled()
                    && u8::from(disp_cap_cnt.vram_write_block()) == bank_num as u8
                    && u8::from(disp_cap_cnt.vram_write_offset()) == offset_num as u8
                {
                    let bytes_len = disp_cap_cnt.pixel_size() * 2;
                    let capture_offset = bank_num * vram::BANK_A_SIZE + offset;
                    let capture_mem = &capture_mem[capture_offset..capture_offset + bytes_len];
                    bank[..bytes_len].copy_from_slice(capture_mem);
                }
            }
        }
    }
}

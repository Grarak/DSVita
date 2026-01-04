use crate::core::graphics::{
    gpu::DISPLAY_HEIGHT,
    gpu_2d::{
        registers_2d::{DispCnt, Gpu2DRegisters},
        Gpu2DEngine::{self, A, B},
    },
    gpu_mem_buf::GpuMemRefs,
};
use crate::utils::HeapMem;
use static_assertions::const_assert;
use std::{hint::assert_unchecked, intrinsics::unlikely, mem};

pub struct Gpu2DMem {
    pub lcdc: &'static [u8],
    pub bg: &'static [u8],
    pub obj: &'static [u8],
    pub pal: &'static [u8],
    pub oam: &'static [u8],
    pub bg_ext_pal: &'static [u8],
    pub obj_ext_pal: &'static [u8],
}

impl Gpu2DMem {
    pub unsafe fn new<const ENGINE: Gpu2DEngine>(refs: &GpuMemRefs) -> Self {
        match ENGINE {
            A => Gpu2DMem {
                lcdc: mem::transmute(refs.lcdc.as_slice()),
                bg: mem::transmute(refs.bg_a.as_slice()),
                obj: mem::transmute(refs.obj_a.as_slice()),
                pal: mem::transmute(refs.pal_a.as_slice()),
                oam: mem::transmute(refs.oam_a.as_slice()),
                bg_ext_pal: mem::transmute(refs.bg_a_ext_pal.as_slice()),
                obj_ext_pal: mem::transmute(refs.obj_a_ext_pal.as_slice()),
            },
            B => Gpu2DMem {
                lcdc: mem::transmute(refs.lcdc.as_slice()),
                bg: mem::transmute(refs.bg_b.as_slice()),
                obj: mem::transmute(refs.obj_b.as_slice()),
                pal: mem::transmute(refs.pal_b.as_slice()),
                oam: mem::transmute(refs.oam_b.as_slice()),
                bg_ext_pal: mem::transmute(refs.bg_b_ext_pal.as_slice()),
                obj_ext_pal: mem::transmute(refs.obj_b_ext_pal.as_slice()),
            },
        }
    }
}

#[derive(Clone)]
#[repr(C)]
pub struct WinBgUbo {
    pub win_h_v: [u32; DISPLAY_HEIGHT * 2],
    pub win_in_out: [u32; DISPLAY_HEIGHT],
}

const_assert!(size_of::<WinBgUbo>() <= 16 * 1024);

#[derive(Clone)]
#[repr(C)]
pub struct BgUbo {
    pub ofs: [u32; DISPLAY_HEIGHT * 4],
    pub x: [i32; DISPLAY_HEIGHT * 2],
    pub y: [i32; DISPLAY_HEIGHT * 2],
    pub pa: [i32; DISPLAY_HEIGHT * 2],
    pub pb: [i32; DISPLAY_HEIGHT * 2],
    pub pc: [i32; DISPLAY_HEIGHT * 2],
    pub pd: [i32; DISPLAY_HEIGHT * 2],
}

const_assert!(size_of::<BgUbo>() <= 16 * 1024);

#[derive(Clone)]
#[repr(C)]
pub struct BlendUbo {
    pub bld_cnts_alphas_ys: [u32; DISPLAY_HEIGHT],
}

const_assert!(size_of::<BlendUbo>() <= 16 * 1024);

#[derive(Clone)]
pub struct Gpu2DRenderRegs {
    pub disp_cnts: [u32; DISPLAY_HEIGHT],
    pub bg_cnts: [u16; DISPLAY_HEIGHT * 4],
    pub win_bg_ubo: WinBgUbo,
    pub bg_ubo: BgUbo,
    pub blend_ubo: BlendUbo,
}

impl Default for Gpu2DRenderRegs {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl Gpu2DRenderRegs {
    fn reset(&mut self) {
        self.disp_cnts = unsafe { mem::zeroed() };
        self.bg_cnts = unsafe { mem::zeroed() };
    }

    fn on_scanline(&mut self, inner: &mut Gpu2DRegisters, line: u8) {
        let line = line as usize;
        unsafe { assert_unchecked(line < DISPLAY_HEIGHT) };

        self.disp_cnts[line] = u32::from(inner.disp_cnt);
        for i in 0..4 {
            self.bg_cnts[line * 4 + i] = u16::from(inner.bg_cnt[i]);
        }

        for i in 0..2 {
            self.win_bg_ubo.win_h_v[line * 2 + i] = inner.win_h[i] as u32 | ((inner.win_v[i] as u32) << 16);
        }
        self.win_bg_ubo.win_in_out[line] = inner.win_in as u32 | ((inner.win_out as u32) << 16);

        for i in 0..4 {
            self.bg_ubo.ofs[line * 4 + i] = (inner.bg_h_ofs[i] as u32) | ((inner.bg_v_ofs[i] as u32) << 16);
        }
        for i in 0..2 {
            self.bg_ubo.x[i * DISPLAY_HEIGHT + line] = inner.bg_x[i];
            self.bg_ubo.y[i * DISPLAY_HEIGHT + line] = inner.bg_y[i];
            self.bg_ubo.pa[i * DISPLAY_HEIGHT + line] = inner.bg_pa[i] as i32;
            self.bg_ubo.pc[i * DISPLAY_HEIGHT + line] = inner.bg_pc[i] as i32;
        }

        if unlikely(line == 0) || inner.bg_x_dirty {
            self.bg_ubo.pb[line] = 0;
            self.bg_ubo.pb[DISPLAY_HEIGHT + line] = 0;
            inner.bg_x_dirty = false;
        } else {
            self.bg_ubo.pb[line] = inner.bg_pb[0] as i32 + self.bg_ubo.pb[line - 1];
            self.bg_ubo.pb[DISPLAY_HEIGHT + line] = inner.bg_pb[1] as i32 + self.bg_ubo.pb[DISPLAY_HEIGHT + line - 1];
        }

        if unlikely(line == 0) || inner.bg_y_dirty {
            self.bg_ubo.pd[line] = 0;
            self.bg_ubo.pd[DISPLAY_HEIGHT + line] = 0;
            inner.bg_y_dirty = false;
        } else {
            self.bg_ubo.pd[line] = inner.bg_pd[0] as i32 + self.bg_ubo.pd[line - 1];
            self.bg_ubo.pd[DISPLAY_HEIGHT + line] = inner.bg_pd[1] as i32 + self.bg_ubo.pd[DISPLAY_HEIGHT + line - 1];
        }

        let eva = (inner.bld_alpha & 0x1F) as u32;
        let evb = (inner.bld_alpha >> 8) as u32;
        self.blend_ubo.bld_cnts_alphas_ys[line] = (inner.bld_cnt as u32) | (eva << 16) | (evb << 21) | ((inner.bld_y as u32) << 26);
    }

    pub fn disp_cnt(&self, line: usize) -> u32 {
        unsafe { *self.disp_cnts.get_unchecked(line) }
    }

    pub fn bg_cnt(&self, line: usize, bg: usize) -> u16 {
        unsafe { *self.bg_cnts.get_unchecked(line * 4 + bg) }
    }

    pub fn ofs(&self, line: usize, bg: usize) -> (u16, u16) {
        let ofs = unsafe { *self.bg_ubo.ofs.get_unchecked(line * 4 + bg) };
        (ofs as u16, (ofs >> 16) as u16)
    }
}

pub struct Gpu2DRenderRegsShared {
    pub regs_a: [HeapMem<Gpu2DRenderRegs>; 2],
    pub regs_b: [HeapMem<Gpu2DRenderRegs>; 2],
    pub has_vram_display: [bool; 2],
}

impl Gpu2DRenderRegsShared {
    pub fn new() -> Self {
        Gpu2DRenderRegsShared {
            regs_a: [HeapMem::default(), HeapMem::default()],
            regs_b: [HeapMem::default(), HeapMem::default()],
            has_vram_display: [false; 2],
        }
    }

    pub fn init(&mut self) {
        self.regs_a[0].reset();
        self.regs_b[0].reset();
        self.has_vram_display[0] = false;
        self.reload_registers();
    }

    pub fn on_scanline(&mut self, inner_a: &mut Gpu2DRegisters, inner_b: &mut Gpu2DRegisters, line: u8) {
        self.regs_a[1].on_scanline(inner_a, line);
        self.regs_b[1].on_scanline(inner_b, line);
        if u8::from(DispCnt::from(self.regs_a[1].disp_cnts[line as usize]).display_mode()) == 2 {
            self.has_vram_display[1] = true;
        }
    }

    pub fn on_scanline_finish(&mut self) {
        self.regs_a.swap(0, 1);
        self.regs_b.swap(0, 1);
        self.has_vram_display[0] = self.has_vram_display[1];
    }

    pub fn reload_registers(&mut self) {
        self.regs_a[1].reset();
        self.regs_b[1].reset();
        self.has_vram_display[1] = false;
    }
}

use crate::emu::gpu::gl::gl_utils::{
    create_fb_color, create_fb_depth, create_mem_texture1d, create_mem_texture2d, create_pal_texture, create_program, create_shader, sub_mem_texture1d, sub_mem_texture2d, sub_pal_texture,
};
use crate::emu::gpu::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::emu::gpu::gpu_2d::Gpu2DEngine::{A, B};
use crate::emu::gpu::gpu_2d::{DispCnt, Gpu2DInner};
use crate::emu::memory::mem::Memory;
use crate::emu::memory::oam::{OamAttrib0, OamAttrib1, OamAttribs, OamGfxMode, OamObjMode};
use crate::emu::memory::{regions, vram};
use crate::presenter::{Presenter, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_TOP_SCREEN};
use crate::utils;
use crate::utils::{HeapMemU32, HeapMemU8, StrErr};
use gl::types::GLuint;
use gl::{TEXTURE0, TEXTURE1};
use std::ops::Deref;
use std::sync::{Condvar, Mutex};
use std::{mem, ptr};

#[derive(Default)]
pub struct GpuMemBuf {
    pub bg_a: HeapMemU8<{ vram::BG_A_SIZE as usize }>,
    pub obj_a: HeapMemU8<{ vram::OBJ_A_SIZE as usize }>,
    pub bg_a_ext_palette: HeapMemU8<{ 32 * 1024 }>,
    pub obj_a_ext_palette: HeapMemU8<{ 8 * 1024 }>,
    pub bg_a_ext_palette_mapped: [bool; 4],
    pub obj_a_ext_palette_mapped: bool,

    pub bg_b: HeapMemU8<{ vram::BG_B_SIZE as usize }>,
    pub obj_b: HeapMemU8<{ vram::OBJ_B_SIZE as usize }>,
    pub bg_b_ext_palette: HeapMemU8<{ 32 * 1024 }>,
    pub obj_b_ext_palette: HeapMemU8<{ 8 * 1024 }>,
    pub bg_b_ext_palette_mapped: [bool; 4],
    pub obj_b_ext_palette_mapped: bool,

    pub pal_a: HeapMemU8<{ regions::STANDARD_PALETTES_SIZE as usize / 2 }>,
    pub pal_b: HeapMemU8<{ regions::STANDARD_PALETTES_SIZE as usize / 2 }>,
    pub oam_a: HeapMemU8<{ regions::OAM_SIZE as usize / 2 }>,
    pub oam_b: HeapMemU8<{ regions::OAM_SIZE as usize / 2 }>,
}

impl GpuMemBuf {
    fn read(&mut self, mem: &mut Memory) {
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

#[derive(Clone)]
struct GpuRegs {
    disp_cnts: [u32; DISPLAY_HEIGHT],
    bg_cnts: [u32; DISPLAY_HEIGHT * 4],
    bg_h_ofs: [u32; 4],
    bg_v_ofs: [u32; 4],
    batch_counts: [u8; DISPLAY_HEIGHT],
    current_batch_count_index: usize,
}

#[repr(C)]
struct BgUbo {
    disp_cnt: u32,
    cnts: [u32; 4],
    h_ofs: [u32; 4],
    v_ofs: [u32; 4],
}

impl Default for GpuRegs {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl GpuRegs {
    fn on_scanline(&mut self, inner: &Gpu2DInner, line: u8) {
        let updated = self.disp_cnts[self.current_batch_count_index] != u32::from(inner.disp_cnt);
        let updated = updated || {
            let mut updated = false;
            for i in 0..4 {
                if self.bg_cnts[i * DISPLAY_HEIGHT + self.current_batch_count_index] != u16::from(inner.bg_cnt[i]) as u32 {
                    updated = true;
                    break;
                }
            }
            updated
        };

        if updated {
            // println!(
            //     "{line} {:x} {:x} {:x} {:x} {:x} {:x} {:x} {:x} {:x} {:x}",
            //     self.disp_cnts[self.current_batch_count_index],
            //     u32::from(inner.disp_cnt),
            //     self.bg_cnts[self.current_batch_count_index],
            //     u16::from(inner.bg_cnt[0]),
            //     self.bg_cnts[DISPLAY_HEIGHT + self.current_batch_count_index],
            //     u16::from(inner.bg_cnt[1]),
            //     self.bg_cnts[2 * DISPLAY_HEIGHT + self.current_batch_count_index],
            //     u16::from(inner.bg_cnt[2]),
            //     self.bg_cnts[3 * DISPLAY_HEIGHT + self.current_batch_count_index],
            //     u16::from(inner.bg_cnt[3]),
            // );
            self.disp_cnts[line as usize] = u32::from(inner.disp_cnt);
            for i in 0..4 {
                self.bg_cnts[i * DISPLAY_HEIGHT + line as usize] = u16::from(inner.bg_cnt[i]) as u32;
            }
            self.current_batch_count_index = line as usize;
        } else {
            self.batch_counts[self.current_batch_count_index] += 1;
        }
    }
}

macro_rules! shader_source {
    ($name:expr) => {{
        #[cfg(target_os = "vita")]
        {
            include_bytes!(concat!("shaders/cg/", $name, ".cg"))
        }
        #[cfg(target_os = "linux")]
        {
            include_bytes!(concat!("shaders/glsl/", $name, ".glsl"))
        }
    }};
}

const VERT_OBJ_SHADER_SRC: &[u8] = shader_source!("vert_obj");
const FRAG_OBJ_SHADER_SRC: &[u8] = shader_source!("frag_obj");

const VERT_BG_SHADER_SRC: &[u8] = shader_source!("vert_bg");
const FRAG_BG_SHADER_SRC: &[u8] = shader_source!("frag_bg");

const fn generate_obj_vertices() -> [f32; 128 * 4 * 2] {
    let mut vertices: [f32; 128 * 4 * 2] = unsafe { mem::zeroed() };
    let mut i = 0;
    while i < vertices.len() {
        // top left
        vertices[i] = 0f32;
        vertices[i + 1] = 0f32;
        // top right
        vertices[i + 2] = 1f32;
        vertices[i + 3] = 0f32;
        // bottom right
        vertices[i + 4] = 1f32;
        vertices[i + 5] = 1f32;
        // bottom left
        vertices[i + 6] = 0f32;
        vertices[i + 7] = 1f32;
        i += 4 * 2;
    }
    vertices
}

const OBJ_VERTICES: [f32; 128 * 4 * 2] = generate_obj_vertices();

const fn generate_oam_indices() -> [u8; 128 * 4] {
    let mut indices: [u8; 128 * 4] = unsafe { mem::zeroed() };
    let mut i = 0;
    while i < indices.len() {
        indices[i] = (i / 4) as u8;
        indices[i + 1] = (i / 4) as u8;
        indices[i + 2] = (i / 4) as u8;
        indices[i + 3] = (i / 4) as u8;
        i += 4;
    }
    indices
}

const OAM_INDICES: [u8; 128 * 4] = generate_oam_indices();

struct Gpu2dShared {
    obj_program: GLuint,
    bg_program: GLuint,
    oam_tex: GLuint,
    bg_tex: GLuint,
    pal_tex: GLuint,
    bg_ubo: GLuint,
    obj_vertices_buf: GLuint,
    obj_vao: GLuint,
    oam_indices: Vec<[u16; 6]>,
}

impl Gpu2dShared {
    fn new() -> Self {
        unsafe {
            let (obj_program, obj_vertices_buf, obj_vao) = {
                let vert_shader = create_shader(VERT_OBJ_SHADER_SRC, gl::VERTEX_SHADER).unwrap();
                let frag_shader = create_shader(FRAG_OBJ_SHADER_SRC, gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(vert_shader);
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);

                let mut vertices_buf = 0;
                gl::GenBuffers(1, ptr::addr_of_mut!(vertices_buf));
                gl::BindBuffer(gl::ARRAY_BUFFER, vertices_buf);
                gl::BufferData(gl::ARRAY_BUFFER, (mem::size_of::<f32>() * OBJ_VERTICES.len()) as _, OBJ_VERTICES.as_ptr() as _, gl::STATIC_DRAW);

                let mut indices_buf = 0;
                gl::GenBuffers(1, ptr::addr_of_mut!(indices_buf));
                gl::BindBuffer(gl::ARRAY_BUFFER, indices_buf);
                gl::BufferData(gl::ARRAY_BUFFER, OAM_INDICES.len() as _, OAM_INDICES.as_ptr() as _, gl::STATIC_DRAW);

                let mut vao = 0;
                gl::GenVertexArrays(1, ptr::addr_of_mut!(vao));
                gl::BindVertexArray(vao);

                gl::BindBuffer(gl::ARRAY_BUFFER, vertices_buf);
                gl::EnableVertexAttribArray(0);
                gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, 0 as _);

                gl::BindBuffer(gl::ARRAY_BUFFER, indices_buf);
                gl::EnableVertexAttribArray(1);
                gl::VertexAttribPointer(1, 1, gl::UNSIGNED_BYTE, gl::FALSE, 0, 0 as _);

                gl::BindVertexArray(0);
                gl::BindBuffer(gl::ARRAY_BUFFER, 0);

                gl::Uniform1i(gl::GetUniformLocation(program, "oamTex\0".as_ptr() as _), 0);
                gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);
                gl::BindAttribLocation(program, 1, "oamIndex\0".as_ptr() as _);

                gl::UseProgram(0);

                (program, vertices_buf, vao)
            };

            let bg_program = {
                let vert_shader = create_shader(VERT_BG_SHADER_SRC, gl::VERTEX_SHADER).unwrap();
                let frag_shader = create_shader(FRAG_BG_SHADER_SRC, gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(vert_shader);
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);
                gl::Uniform1i(gl::GetUniformLocation(program, "bgTex\0".as_ptr() as _), 0);
                gl::Uniform1i(gl::GetUniformLocation(program, "palTex\0".as_ptr() as _), 1);
                gl::UseProgram(0);

                program
            };

            let oam_tex = create_mem_texture1d(regions::OAM_SIZE / 2);
            let bg_tex = create_mem_texture2d(1024, 512);
            let pal_tex = create_pal_texture(regions::STANDARD_PALETTES_SIZE / 2);

            let mut bg_ubo = 0;
            gl::GenBuffers(1, ptr::addr_of_mut!(bg_ubo));

            Gpu2dShared {
                obj_program,
                bg_program,
                oam_tex,
                bg_tex,
                pal_tex,
                bg_ubo,
                obj_vertices_buf,
                obj_vao,
                oam_indices: Vec::new(),
            }
        }
    }

    unsafe fn draw_objects(&mut self, regs: &GpuRegs, mem_buf: &GpuMemBuf, from_line: u8, to_line: u8) {
        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line as usize]);
        if !disp_cnt.screen_display_obj() {
            return;
        }

        if disp_cnt.obj_window_display_flag() {
            self.assemble_oam::<true>(mem_buf, from_line, to_line);
        } else {
            self.assemble_oam::<false>(mem_buf, from_line, to_line);
        }

        if self.oam_indices.is_empty() {
            return;
        }

        gl::DrawElements(gl::TRIANGLES, (6 * self.oam_indices.len()) as _, gl::UNSIGNED_SHORT, self.oam_indices.as_ptr() as _);
        gl::Flush();
    }

    unsafe fn draw_bg(&self, regs: &GpuRegs, mem_buf: &GpuMemBuf, from_line: u8, to_line: u8) {
        let vertices = [
            // top left
            -1f32,
            from_line as f32,
            // top right
            1f32,
            from_line as f32,
            // bottom right
            1f32,
            to_line as f32,
            // bottom left
            -1f32,
            to_line as f32,
        ];
        let bg_ubo = BgUbo {
            disp_cnt: regs.disp_cnts[from_line as usize],
            cnts: [
                regs.bg_cnts[from_line as usize],
                regs.bg_cnts[DISPLAY_HEIGHT + from_line as usize],
                regs.bg_cnts[2 * DISPLAY_HEIGHT + from_line as usize],
                regs.bg_cnts[3 * DISPLAY_HEIGHT + from_line as usize],
            ],
            h_ofs: regs.bg_h_ofs,
            v_ofs: regs.bg_v_ofs,
        };
        gl::BindBuffer(gl::UNIFORM_BUFFER, self.bg_ubo);
        gl::BufferData(gl::UNIFORM_BUFFER, mem::size_of::<BgUbo>() as _, ptr::addr_of!(bg_ubo) as _, gl::DYNAMIC_DRAW);
        gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, self.bg_ubo);

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, vertices.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

        gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
    }

    unsafe fn draw(&mut self, regs: &GpuRegs, mem_buf: &GpuMemBuf) {
        let backdrop = utils::read_from_mem::<u16>(mem_buf.pal_a.deref(), 0);
        let (r, g, b, _) = Self::rgb5_to_float8(backdrop);
        gl::ClearColor(r, g, b, 1f32);

        macro_rules! draw_scanlines {
            ($draw_fn:expr) => {{
                let mut line = 0;
                while line < DISPLAY_HEIGHT {
                    let batch_count = regs.batch_counts[line];
                    let from_line = line as u8;
                    let to_line = line as u8 + batch_count as u8 + 1;
                    $draw_fn(self, regs, mem_buf, from_line, to_line);
                    line = to_line as usize;
                }
            }};
        }

        gl::BindTexture(gl::TEXTURE_2D, self.oam_tex);
        sub_mem_texture1d(regions::OAM_SIZE / 2, mem_buf.oam_a.deref());

        gl::BindTexture(gl::TEXTURE_2D, self.bg_tex);
        sub_mem_texture2d(1024, 512, mem_buf.bg_a.deref());

        gl::BindTexture(gl::TEXTURE_2D, self.pal_tex);
        sub_pal_texture(regions::STANDARD_PALETTES_SIZE / 2, mem_buf.pal_a.deref());

        gl::BindTexture(gl::TEXTURE_2D, 0);

        {
            gl::UseProgram(self.obj_program);

            gl::BindVertexArray(self.obj_vao);

            gl::ActiveTexture(TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.oam_tex);
            draw_scanlines!(Self::draw_objects);

            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindVertexArray(0);
        }

        {
            gl::UseProgram(self.bg_program);
            gl::ActiveTexture(TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.bg_tex);

            gl::ActiveTexture(TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, self.pal_tex);

            draw_scanlines!(Self::draw_bg);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        gl::UseProgram(0);
    }

    fn assemble_oam<const OBJ_WINDOW: bool>(&mut self, mem_buf: &GpuMemBuf, from_line: u8, to_line: u8) {
        const OAM_COUNT: usize = regions::OAM_SIZE as usize / 2 / mem::size_of::<OamAttribs>();
        let oams = unsafe { (mem_buf.oam_a.as_ptr() as *const [OamAttribs; OAM_COUNT]).as_ref().unwrap_unchecked() };

        self.oam_indices.clear();
        for (i, oam) in oams.iter().enumerate() {
            let attrib0 = OamAttrib0::from(oam.attr0);
            let obj_mode = attrib0.get_obj_mode();
            if obj_mode == OamObjMode::Disabled {
                continue;
            }
            let gfx_mode = attrib0.get_gfx_mode();
            if !OBJ_WINDOW && gfx_mode == OamGfxMode::Window {
                continue;
            }

            let attrib1 = OamAttrib1::from(oam.attr1);
            let mut x = u16::from(attrib1.x()) as i32;
            if x >= DISPLAY_WIDTH as i32 {
                x -= 512;
            }
            let mut y = attrib0.y() as i32;
            if y >= DISPLAY_HEIGHT as i32 {
                y -= 256;
            }

            let (width, height) = match (u8::from(attrib0.shape()) << 2) | u8::from(attrib1.size()) {
                0x0 => (8, 8),
                0x1 => (16, 16),
                0x2 => (32, 32),
                0x3 => (64, 64),
                0x4 => (16, 8),
                0x5 => (32, 8),
                0x6 => (32, 16),
                0x7 => (64, 32),
                0x8 => (8, 16),
                0x9 => (8, 32),
                0xA => (16, 32),
                0xB => (32, 64),
                _ => continue,
            };

            if x + width < 0 || y + height < from_line as i32 || x >= DISPLAY_WIDTH as i32 || y >= to_line as i32 {
                continue;
            }

            if gfx_mode == OamGfxMode::Bitmap {
                todo!()
            }

            if obj_mode != OamObjMode::Normal {
                todo!()
            }

            if attrib0.is_8bit() {
                todo!()
            }

            let index_base = (i * 4) as u16;
            self.oam_indices.push([index_base, index_base + 1, index_base + 2, index_base, index_base + 2, index_base + 3]);
        }
    }

    fn rgb5_to_float8(color: u16) -> (f32, f32, f32, f32) {
        let r = (color & 0x1F) as f32;
        let g = ((color >> 5) & 0x1F) as f32;
        let b = ((color >> 10) & 0x1F) as f32;
        let a = ((color >> 15) & 1) as f32;
        (r / 31f32, g / 31f32, b / 31f32, a)
    }
}

struct Fbo {
    color: GLuint,
    depth: GLuint,
    fbo: GLuint,
}

impl Fbo {
    fn new(width: u32, height: u32) -> Result<Self, StrErr> {
        unsafe {
            let color = create_fb_color(width, height);
            let depth = create_fb_depth(width, height);

            let mut fbo = 0;
            gl::GenFramebuffers(1, ptr::addr_of_mut!(fbo));
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, color, 0);
            gl::FramebufferRenderbuffer(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::RENDERBUFFER, depth);

            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            if status != gl::FRAMEBUFFER_COMPLETE {
                Err(StrErr::new(format!("Failed to create fbo: {status}")))
            } else {
                Ok(Fbo { color, depth, fbo })
            }
        }
    }
}

pub struct Gpu2dRenderer {
    regs_a: [GpuRegs; 2],
    regs_b: [GpuRegs; 2],
    mem_buf: GpuMemBuf,
    mem_buf_swap: GpuMemBuf,
    drawing: Mutex<bool>,
    drawing_condvar: Condvar,
    shared: Gpu2dShared,
    fb_a: Fbo,
    fb_b: Fbo,
    read_buf: HeapMemU32<{ DISPLAY_WIDTH * DISPLAY_HEIGHT }>,
}

impl Gpu2dRenderer {
    pub fn new() -> Self {
        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::DepthFunc(gl::LESS);
        }

        let shared = Gpu2dShared::new();
        let fb_a = Fbo::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32).unwrap();
        let fb_b = Fbo::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32).unwrap();

        Gpu2dRenderer {
            regs_a: [GpuRegs::default(), GpuRegs::default()],
            regs_b: [GpuRegs::default(), GpuRegs::default()],
            mem_buf: GpuMemBuf::default(),
            mem_buf_swap: GpuMemBuf::default(),
            drawing: Mutex::new(false),
            drawing_condvar: Condvar::new(),
            shared,
            fb_a,
            fb_b,
            read_buf: HeapMemU32::new(),
        }
    }

    pub fn on_scanline(&mut self, inner_a: &Gpu2DInner, inner_b: &Gpu2DInner, line: u8) {
        self.regs_a[1].on_scanline(inner_a, line);
        self.regs_b[1].on_scanline(inner_b, line);
    }

    pub fn on_frame(&mut self, inner_a: &Gpu2DInner, inner_b: &Gpu2DInner, mem: &mut Memory) {
        self.mem_buf_swap.read(mem);
        for i in 0..4 {
            self.regs_a[1].bg_h_ofs[i] = inner_a.bg_h_ofs[i] as u32;
            self.regs_a[1].bg_v_ofs[i] = inner_a.bg_v_ofs[i] as u32;
            self.regs_b[1].bg_h_ofs[i] = inner_b.bg_h_ofs[i] as u32;
            self.regs_b[1].bg_v_ofs[i] = inner_b.bg_v_ofs[i] as u32;
        }
    }

    pub fn reload_registers(&mut self) {
        self.regs_a[1] = GpuRegs::default();
        self.regs_b[1] = GpuRegs::default();
    }

    pub fn start_drawing(&mut self) {
        let mut drawing = self.drawing.lock().unwrap();
        // let mut drawing = self.drawing_condvar.wait_while(drawing, |drawing| *drawing).unwrap();

        if !*drawing {
            self.mem_buf.bg_a.copy_from_slice(self.mem_buf_swap.bg_a.deref());
            self.mem_buf.obj_a.copy_from_slice(self.mem_buf_swap.obj_a.deref());
            self.mem_buf.bg_a_ext_palette.copy_from_slice(self.mem_buf_swap.bg_a_ext_palette.deref());
            self.mem_buf.obj_a_ext_palette.copy_from_slice(self.mem_buf_swap.obj_a_ext_palette.deref());
            self.mem_buf.bg_a_ext_palette_mapped = self.mem_buf_swap.bg_a_ext_palette_mapped;
            self.mem_buf.obj_a_ext_palette_mapped = self.mem_buf_swap.obj_a_ext_palette_mapped;

            self.mem_buf.bg_b.copy_from_slice(self.mem_buf_swap.bg_b.deref());
            self.mem_buf.obj_b.copy_from_slice(self.mem_buf_swap.obj_b.deref());
            self.mem_buf.bg_b_ext_palette.copy_from_slice(self.mem_buf_swap.bg_b_ext_palette.deref());
            self.mem_buf.obj_b_ext_palette.copy_from_slice(self.mem_buf_swap.obj_b_ext_palette.deref());
            self.mem_buf.bg_b_ext_palette_mapped = self.mem_buf_swap.bg_b_ext_palette_mapped;
            self.mem_buf.obj_b_ext_palette_mapped = self.mem_buf_swap.obj_b_ext_palette_mapped;

            self.mem_buf.pal_a.copy_from_slice(self.mem_buf_swap.pal_a.deref());
            self.mem_buf.pal_b.copy_from_slice(self.mem_buf_swap.pal_b.deref());

            self.mem_buf.oam_a.copy_from_slice(self.mem_buf_swap.oam_a.deref());
            self.mem_buf.oam_b.copy_from_slice(self.mem_buf_swap.oam_b.deref());

            self.regs_a[0] = self.regs_a[1].clone();
            self.regs_b[0] = self.regs_b[1].clone();

            *drawing = true;
            self.drawing_condvar.notify_one();
        }
    }

    pub unsafe fn draw(&mut self, presenter: &mut Presenter) {
        {
            let drawing = self.drawing.lock().unwrap();
            let _drawing = self.drawing_condvar.wait_while(drawing, |drawing| !*drawing).unwrap();
        }

        gl::BindFramebuffer(gl::FRAMEBUFFER, self.fb_a.fbo);
        gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
        gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

        self.shared.draw(&self.regs_a[0], &self.mem_buf);

        // gl::ReadPixels(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, gl::RGBA, gl::UNSIGNED_BYTE, self.read_buf.as_mut_ptr() as _);

        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.fb_a.fbo);
        gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);

        gl::ClearColor(0f32, 0f32, 0f32, 1f32);
        gl::Clear(gl::COLOR_BUFFER_BIT);

        gl::BlitFramebuffer(
            0,
            0,
            DISPLAY_WIDTH as _,
            DISPLAY_HEIGHT as _,
            PRESENTER_SUB_TOP_SCREEN.x as _,
            PRESENTER_SUB_TOP_SCREEN.y as _,
            PRESENTER_SUB_TOP_SCREEN.width as _,
            (PRESENTER_SUB_TOP_SCREEN.y + PRESENTER_SUB_TOP_SCREEN.height) as _,
            gl::COLOR_BUFFER_BIT,
            gl::NEAREST,
        );

        presenter.gl_swap_window();

        // println!(
        //     "{:x} {:x} {:x} {:x} {:x}",
        //     self.read_buf[0],
        //     self.read_buf[1],
        //     utils::read_from_mem::<u16>(self.mem_buf.oam_a.deref(), 0),
        //     utils::read_from_mem::<u16>(self.mem_buf.oam_a.deref(), 2),
        //     utils::read_from_mem::<u16>(self.mem_buf.oam_a.deref(), 4),
        // );

        {
            let mut drawing = self.drawing.lock().unwrap();
            *drawing = false;
            self.drawing_condvar.notify_one();
        }
    }
}

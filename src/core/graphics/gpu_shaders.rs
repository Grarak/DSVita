use crate::core::graphics::gl_utils::{create_program, shader_source};
use gl::types::{GLenum, GLint, GLuint};

pub struct Gpu2DBgShaderPrograms {
    pub affine: GLuint,
    pub affine_extended: GLuint,
    pub bitmap: GLuint,
    pub display_3d: GLuint,
    pub text_4bpp: GLuint,
    pub text_8bpp: GLuint,
}

impl Gpu2DBgShaderPrograms {
    unsafe fn new<F: FnMut(&str, &str, GLenum) -> GLuint>(mut create_shader: F) -> Self {
        let bg_vert_shader = create_shader("bg", shader_source!("gpu_2d/shaders", "bg_vert"), gl::VERTEX_SHADER);
        let bg_vert_affine_shader = create_shader("bg affine", shader_source!("gpu_2d/shaders", "bg_vert_affine"), gl::VERTEX_SHADER);
        let bg_vert_bitmap_shader = create_shader("bg bitmap", shader_source!("gpu_2d/shaders", "bg_vert_bitmap"), gl::VERTEX_SHADER);

        let frag_common_shader_src = shader_source!("gpu_2d/shaders", "bg_frag_common").to_string();
        let mut create_bg_frag_shader = |name: &str, src: &str| create_shader(name, &(frag_common_shader_src.clone() + src), gl::FRAGMENT_SHADER);

        let frag_affine_shader = create_bg_frag_shader("bg affine", shader_source!("gpu_2d/shaders", "bg_frag_affine"));
        let frag_affine_extended_shader = create_bg_frag_shader("bg affine extended", shader_source!("gpu_2d/shaders", "bg_frag_affine_extended"));
        let frag_bitmap_shader = create_bg_frag_shader("bg bitmap", shader_source!("gpu_2d/shaders", "bg_frag_bitmap"));
        let frag_display_3d_shader = create_bg_frag_shader("bg display 3d", shader_source!("gpu_2d/shaders", "bg_frag_display_3d"));
        let frag_text_4bpp_shader = create_bg_frag_shader("bg text 4bpp", shader_source!("gpu_2d/shaders", "bg_frag_text_4bpp"));
        let frag_text_8bpp_shader = create_bg_frag_shader("bg text 8bpp", shader_source!("gpu_2d/shaders", "bg_frag_text_8bpp"));

        let affine = create_program(&[bg_vert_affine_shader, frag_affine_shader]).unwrap();
        let affine_extended = create_program(&[bg_vert_affine_shader, frag_affine_extended_shader]).unwrap();
        let bitmap = create_program(&[bg_vert_bitmap_shader, frag_bitmap_shader]).unwrap();
        let display_3d = create_program(&[bg_vert_shader, frag_display_3d_shader]).unwrap();
        let text_4bpp = create_program(&[bg_vert_shader, frag_text_4bpp_shader]).unwrap();
        let text_8bpp = create_program(&[bg_vert_shader, frag_text_8bpp_shader]).unwrap();

        gl::DeleteShader(bg_vert_shader);
        gl::DeleteShader(bg_vert_affine_shader);
        gl::DeleteShader(bg_vert_bitmap_shader);
        gl::DeleteShader(frag_affine_shader);
        gl::DeleteShader(frag_affine_extended_shader);
        gl::DeleteShader(frag_bitmap_shader);
        gl::DeleteShader(frag_display_3d_shader);
        gl::DeleteShader(frag_text_4bpp_shader);
        gl::DeleteShader(frag_text_8bpp_shader);

        Gpu2DBgShaderPrograms {
            affine,
            affine_extended,
            bitmap,
            display_3d,
            text_4bpp,
            text_8bpp,
        }
    }

    const fn count() -> usize {
        9
    }
}

#[derive(Copy, Clone)]
pub struct Gpu3DShaderPrograms {
    pub program: GLuint,
    pub polygon_attrs: GLint,
    pub tex_image_param: GLint,
    pub screen_width: GLint,
}

impl Gpu3DShaderPrograms {
    unsafe fn new<F: FnMut(&str, &str, GLenum) -> GLuint>(vertex_shader: GLuint, frag_shader_src: &str, create_shader: &mut F) -> Self {
        let frag_shader = create_shader("render 3d opaque", frag_shader_src, gl::FRAGMENT_SHADER);
        let program = create_program(&[vertex_shader, frag_shader]).unwrap();
        gl::DeleteShader(frag_shader);

        let mut previous_program = 0;
        gl::GetIntegerv(gl::CURRENT_PROGRAM, &mut previous_program);

        gl::UseProgram(program);

        gl::BindAttribLocation(program, 0, c"position".as_ptr() as _);
        gl::BindAttribLocation(program, 1, c"texCoords".as_ptr() as _);
        gl::BindAttribLocation(program, 2, c"viewport".as_ptr() as _);
        gl::BindAttribLocation(program, 3, c"color".as_ptr() as _);
        gl::BindAttribLocation(program, 4, c"texSize".as_ptr() as _);

        gl::Uniform1i(gl::GetUniformLocation(program, c"tex".as_ptr() as _), 0);

        let polygon_attrs = gl::GetUniformLocation(program, c"polygonAttrsF".as_ptr() as _);
        let tex_image_param = gl::GetUniformLocation(program, c"texImageParamF".as_ptr() as _);
        let screen_width = gl::GetUniformLocation(program, c"screenWidth".as_ptr() as _);

        gl::UseProgram(previous_program as _);

        Gpu3DShaderPrograms {
            program,
            polygon_attrs,
            tex_image_param,
            screen_width,
        }
    }

    const fn count() -> usize {
        1
    }
}

#[derive(Copy, Clone)]
pub struct Gpu3DShaderDepthPrograms {
    z: Gpu3DShaderPrograms,
    w: Gpu3DShaderPrograms,
}

impl Gpu3DShaderDepthPrograms {
    unsafe fn new<F: FnMut(&str, &str, GLenum) -> GLuint>(vertex_shader: GLuint, frag_shader_src: &str, create_shader: &mut F) -> Self {
        let z = Gpu3DShaderPrograms::new(vertex_shader, frag_shader_src, create_shader);
        let w = Gpu3DShaderPrograms::new(vertex_shader, &("#define W_DEPTH_BUFFER\n".to_string() + frag_shader_src), create_shader);

        Gpu3DShaderDepthPrograms { z, w }
    }

    pub fn get_program(&self, w_depth_buffer: bool) -> &Gpu3DShaderPrograms {
        if w_depth_buffer {
            &self.w
        } else {
            &self.z
        }
    }

    const fn count() -> usize {
        2 * Gpu3DShaderPrograms::count()
    }
}

pub struct Gpu2DObjShaderProgram {
    pub sprite_4bpp: GLuint,
    pub sprite_8bpp: GLuint,
    pub bitmap: GLuint,
}

impl Gpu2DObjShaderProgram {
    unsafe fn new<F: FnMut(&str, &str, GLenum) -> GLuint>(create_shader: &mut F) -> Self {
        let vertex_shader = create_shader("obj", shader_source!("gpu_2d/shaders", "obj_vert"), gl::VERTEX_SHADER);
        let frag_src = shader_source!("gpu_2d/shaders", "obj_frag");

        let sprite_4bpp_frag = create_shader("obj", &frag_src, gl::FRAGMENT_SHADER);
        let sprite_4bpp = create_program(&[vertex_shader, sprite_4bpp_frag]).unwrap();
        gl::DeleteShader(sprite_4bpp_frag);

        let sprite_8bpp_frag = create_shader("obj", &("#define BPP8\n".to_string() + &frag_src), gl::FRAGMENT_SHADER);
        let sprite_8bpp = create_program(&[vertex_shader, sprite_8bpp_frag]).unwrap();
        gl::DeleteShader(sprite_4bpp_frag);

        let bitmap_frag = create_shader("obj", &("#define BITMAP\n".to_string() + &frag_src), gl::FRAGMENT_SHADER);
        let bitmap = create_program(&[vertex_shader, bitmap_frag]).unwrap();
        gl::DeleteShader(bitmap_frag);

        gl::DeleteShader(vertex_shader);

        Gpu2DObjShaderProgram { sprite_4bpp, sprite_8bpp, bitmap }
    }

    const fn count() -> usize {
        4
    }
}

pub struct Gpu2DBlendProgram {
    pub top: GLuint,
    pub bottom: GLuint,
}

impl Gpu2DBlendProgram {
    unsafe fn new<F: FnMut(&str, &str, GLenum) -> GLuint>(create_shader: &mut F) -> Self {
        let vertex_shader = create_shader("blend", shader_source!("gpu_2d/shaders", "blend_vert"), gl::VERTEX_SHADER);
        let frag_src = shader_source!("gpu_2d/shaders", "blend_frag");

        let top_frag = create_shader("blend top", &("#define BLEND_3D\n".to_string() + frag_src), gl::FRAGMENT_SHADER);
        let top_shader = create_program(&[vertex_shader, top_frag]).unwrap();
        gl::DeleteShader(top_frag);

        let bottom_frag = create_shader("blend bottom", frag_src, gl::FRAGMENT_SHADER);
        let bottom_shader = create_program(&[vertex_shader, bottom_frag]).unwrap();
        gl::DeleteShader(bottom_frag);

        gl::DeleteShader(vertex_shader);

        Gpu2DBlendProgram {
            top: top_shader,
            bottom: bottom_shader,
        }
    }

    const fn count() -> usize {
        3
    }
}

pub struct GpuShadersPrograms {
    pub bg: Gpu2DBgShaderPrograms,
    pub obj: Gpu2DObjShaderProgram,
    pub win: GLuint,
    pub blend: Gpu2DBlendProgram,
    pub blend_3d: GLuint,
    pub vram_display: GLuint,
    pub render_3d: Gpu3DShaderDepthPrograms,
    pub text: GLuint,
    pub capture: GLuint,
    pub merge: GLuint,
    pub ra: GLuint,
}

impl GpuShadersPrograms {
    pub unsafe fn new<F: FnMut(&str, &str, GLenum) -> GLuint>(mut create_shader: F) -> Self {
        let mut create_program = |name: &str, vertex_src: &str, frag_src: &str| unsafe {
            let vert_shader = create_shader(name, vertex_src, gl::VERTEX_SHADER);
            let frag_shader = create_shader(name, frag_src, gl::FRAGMENT_SHADER);
            let program = create_program(&[vert_shader, frag_shader]).unwrap();
            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);
            program
        };

        let win = create_program("win", shader_source!("gpu_2d/shaders", "win_bg_vert"), shader_source!("gpu_2d/shaders", "win_bg_frag"));
        let vram_display = create_program(
            "vram display",
            shader_source!("gpu_2d/shaders", "vram_display_vert"),
            shader_source!("gpu_2d/shaders", "vram_display_frag"),
        );

        let text = create_program("text", shader_source!("text_vert"), shader_source!("text_frag"));
        let capture = create_program("capture", shader_source!("capture_vert"), shader_source!("capture_frag"));
        let merge = create_program("merge", shader_source!("merge_vert"), shader_source!("merge_frag"));
        let blend_3d = create_program("blend 3d", shader_source!("gpu_2d/shaders", "blend_3d_vert"), shader_source!("gpu_2d/shaders", "blend_3d_frag"));
        let ra = create_program("ra", shader_source!("ra_vert"), shader_source!("ra_frag"));

        let obj = Gpu2DObjShaderProgram::new(&mut create_shader);

        let render_3d_vertex_shader = create_shader("render 3d", shader_source!("gpu_3d/shaders", "render_vert"), gl::VERTEX_SHADER);
        let render_3d = Gpu3DShaderDepthPrograms::new(render_3d_vertex_shader, shader_source!("gpu_3d/shaders", "render_frag"), &mut create_shader);
        gl::DeleteShader(render_3d_vertex_shader);

        GpuShadersPrograms {
            bg: Gpu2DBgShaderPrograms::new(&mut create_shader),
            obj,
            win,
            blend: Gpu2DBlendProgram::new(&mut create_shader),
            blend_3d,
            vram_display,
            render_3d,
            text,
            capture,
            merge,
            ra,
        }
    }

    pub const fn count() -> usize {
        15 + Gpu3DShaderDepthPrograms::count() + Gpu2DBgShaderPrograms::count() + Gpu2DObjShaderProgram::count() + Gpu2DBlendProgram::count()
    }
}

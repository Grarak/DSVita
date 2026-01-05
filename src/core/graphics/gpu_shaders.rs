use crate::core::graphics::gl_utils::{create_program, shader_source};
use gl::types::{GLenum, GLuint};

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
        let mut create_bg_frag_shader = |src: &str| create_shader("bg affine", &(frag_common_shader_src.clone() + src), gl::FRAGMENT_SHADER);

        let frag_affine_shader = create_bg_frag_shader(shader_source!("gpu_2d/shaders", "bg_frag_affine"));
        let frag_affine_extended_shader = create_bg_frag_shader(shader_source!("gpu_2d/shaders", "bg_frag_affine_extended"));
        let frag_bitmap_shader = create_bg_frag_shader(shader_source!("gpu_2d/shaders", "bg_frag_bitmap"));
        let frag_display_3d_shader = create_bg_frag_shader(shader_source!("gpu_2d/shaders", "bg_frag_display_3d"));
        let frag_text_4bpp_shader = create_bg_frag_shader(shader_source!("gpu_2d/shaders", "bg_frag_text_4bpp"));
        let frag_text_8bpp_shader = create_bg_frag_shader(shader_source!("gpu_2d/shaders", "bg_frag_text_8bpp"));

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

pub struct GpuShadersPrograms {
    pub bg: Gpu2DBgShaderPrograms,
    pub obj: GLuint,
    pub win: GLuint,
    pub blend: GLuint,
    pub vram_display: GLuint,
    pub render_3d: GLuint,
    pub text: GLuint,
    pub capture: GLuint,
    pub merge: GLuint,
}

impl GpuShadersPrograms {
    pub fn new<F: FnMut(&str, &str, GLenum) -> GLuint>(mut create_shader: F) -> Self {
        let mut create_program = |name: &str, vertex_src: &str, frag_src: &str| unsafe {
            let vert_shader = create_shader(name, vertex_src, gl::VERTEX_SHADER);
            let frag_shader = create_shader(name, frag_src, gl::FRAGMENT_SHADER);
            let program = create_program(&[vert_shader, frag_shader]).unwrap();
            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);
            program
        };

        let obj = create_program("obj", shader_source!("gpu_2d/shaders", "obj_vert"), shader_source!("gpu_2d/shaders", "obj_frag"));
        let win = create_program("win", shader_source!("gpu_2d/shaders", "win_bg_vert"), shader_source!("gpu_2d/shaders", "win_bg_frag"));
        let blend = create_program("blend", shader_source!("gpu_2d/shaders", "blend_vert"), shader_source!("gpu_2d/shaders", "blend_frag"));
        let vram_display = create_program(
            "vram display",
            shader_source!("gpu_2d/shaders", "vram_display_vert"),
            shader_source!("gpu_2d/shaders", "vram_display_frag"),
        );
        let render_3d = create_program("render 3d", shader_source!("gpu_3d/shaders", "render_vert"), shader_source!("gpu_3d/shaders", "render_frag"));
        let text = create_program("text", shader_source!("text_vert"), shader_source!("text_frag"));
        let capture = create_program("capture", shader_source!("capture_vert"), shader_source!("capture_frag"));
        let merge = create_program("merge", shader_source!("merge_vert"), shader_source!("merge_frag"));

        GpuShadersPrograms {
            bg: unsafe { Gpu2DBgShaderPrograms::new(create_shader) },
            obj,
            win,
            blend,
            vram_display,
            render_3d,
            text,
            capture,
            merge,
        }
    }

    pub const fn count() -> usize {
        16 + Gpu2DBgShaderPrograms::count()
    }
}

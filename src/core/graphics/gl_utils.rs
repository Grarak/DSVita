use crate::presenter::Presenter;
use crate::utils::StrErr;
use gl::types::{GLenum, GLuint};
use std::ptr;

macro_rules! shader_source {
    ($name:expr) => {{
        #[cfg(target_os = "vita")]
        {
            include_str!(concat!("shaders/cg/", $name, ".cg"))
        }
        #[cfg(target_os = "linux")]
        {
            include_str!(concat!("shaders/glsl/", $name, ".glsl"))
        }
    }};
}

pub(in crate::core::graphics) use shader_source;

pub unsafe fn create_shader(shader_src: &str, typ: GLenum) -> Result<GLuint, StrErr> {
    let shader = gl::CreateShader(typ);
    if shader == 0 {
        return Err(StrErr::new("Failed to create shader"));
    }

    let src_ptr = shader_src.as_ptr();
    let src_len = shader_src.len();
    gl::ShaderSource(shader, 1, ptr::addr_of!(src_ptr) as _, ptr::addr_of!(src_len) as _);
    gl::CompileShader(shader);
    let mut compiled = 0;
    gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut compiled);

    if compiled == 0 {
        let mut info_len = 0;
        gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut info_len);

        if info_len > 1 {
            let mut info = Vec::new();
            Vec::resize(&mut info, info_len as usize, 0u8);
            gl::GetShaderInfoLog(shader, info_len, ptr::null_mut(), info.as_mut_ptr() as _);
            gl::DeleteShader(shader);
            return Err(StrErr::new(String::from_utf8(info).unwrap()));
        }

        gl::DeleteShader(shader);
        return Err(StrErr::new("Failed to compile shader"));
    }
    Ok(shader)
}

pub unsafe fn create_program(shaders: &[GLuint]) -> Result<GLuint, StrErr> {
    let program = gl::CreateProgram();
    for shader in shaders {
        gl::AttachShader(program, *shader);
    }
    gl::LinkProgram(program);

    let mut linked = 0;
    gl::GetProgramiv(program, gl::LINK_STATUS, &mut linked);
    if linked == 0 {
        let mut info_len = 0;
        gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut info_len);
        if info_len > 1 {
            let mut info = Vec::new();
            Vec::resize(&mut info, info_len as usize, 0u8);
            gl::GetProgramInfoLog(program, info_len, ptr::null_mut(), info.as_mut_ptr() as _);
            gl::DeleteProgram(program);
            return Err(StrErr::new(String::from_utf8(info).unwrap()));
        }

        gl::DeleteProgram(program);
        return Err(StrErr::new("Failed to link program"));
    }
    Ok(program)
}

pub unsafe fn create_mem_texture1d(size: u32) -> GLuint {
    let mut tex = 0;
    gl::GenTextures(1, &mut tex);
    gl::BindTexture(gl::TEXTURE_2D, tex);
    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, (size / 4) as _, 1, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
    gl::BindTexture(gl::TEXTURE_2D, 0);
    tex
}

pub unsafe fn create_mem_texture2d(width: u32, height: u32) -> GLuint {
    let mut tex = 0;
    gl::GenTextures(1, &mut tex);
    gl::BindTexture(gl::TEXTURE_2D, tex);
    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, (width / 2) as _, (height / 2) as _, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
    gl::BindTexture(gl::TEXTURE_2D, 0);
    tex
}

pub unsafe fn create_pal_texture1d(size: u32) -> GLuint {
    if cfg!(target_os = "linux") {
        create_mem_texture1d(size)
    } else {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, (size / 2) as _, 1, 0, gl::RGBA, gl::UNSIGNED_SHORT_1_5_5_5_REV, ptr::null());
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        tex
    }
}

pub unsafe fn create_pal_texture2d(width: u32, height: u32) -> GLuint {
    if cfg!(target_os = "linux") {
        create_mem_texture2d(width, height)
    } else {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as _,
            512,
            (width * height / 2 / 512) as _,
            0,
            gl::RGBA,
            gl::UNSIGNED_SHORT_1_5_5_5_REV,
            ptr::null(),
        );
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        tex
    }
}

pub unsafe fn sub_mem_texture1d(size: u32, data: *const u8) {
    gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, (size / 4) as _, 1, gl::RGBA, gl::UNSIGNED_BYTE, data as _);
}

pub unsafe fn sub_mem_texture2d(width: u32, height: u32, data: *const u8) {
    gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, (width / 2) as _, (height / 2) as _, gl::RGBA, gl::UNSIGNED_BYTE, data as _);
}

pub unsafe fn sub_pal_texture1d(size: u32, data: *const u8) {
    if cfg!(target_os = "linux") {
        sub_mem_texture1d(size, data)
    } else {
        gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, (size / 2) as _, 1, gl::RGBA, gl::UNSIGNED_SHORT_1_5_5_5_REV, data as _);
    }
}

pub unsafe fn sub_pal_texture2d(width: u32, height: u32, data: *const u8) {
    if cfg!(target_os = "linux") {
        sub_mem_texture2d(width, height, data)
    } else {
        gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, 512, (width * height / 2 / 512) as _, gl::RGBA, gl::UNSIGNED_SHORT_1_5_5_5_REV, data as _);
    }
}

pub unsafe fn create_fb_color(width: u32, height: u32) -> GLuint {
    let mut tex = 0;
    gl::GenTextures(1, &mut tex);
    gl::BindTexture(gl::TEXTURE_2D, tex);
    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, width as _, height as _, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
    gl::BindTexture(gl::TEXTURE_2D, 0);
    tex
}

pub unsafe fn create_fb_depth_tex(fbo: GLuint, width: u32, height: u32) -> GLuint {
    gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
    if cfg!(target_os = "linux") {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::DEPTH_COMPONENT32F as _, width as _, height as _, 0, gl::DEPTH_COMPONENT, gl::FLOAT, ptr::null());
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
        gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::TEXTURE_2D, tex, 0);
        tex
    } else {
        let mut buf = 0;
        gl::GenRenderbuffers(1, &mut buf);
        gl::BindRenderbuffer(gl::RENDERBUFFER, buf);
        gl::RenderbufferStorage(gl::RENDERBUFFER, gl::DEPTH_COMPONENT, width as _, height as _);
        gl::FramebufferRenderbuffer(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::RENDERBUFFER, buf);
        Presenter::gl_create_depth_tex()
    }
}

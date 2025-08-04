use crate::core::graphics::gl_utils::{create_program, create_shader, shader_source};
use gl::types::GLuint;
use glyph_brush::ab_glyph::FontRef;
use glyph_brush::{BrushAction, BrushError, Extra, GlyphBrush, GlyphBrushBuilder, Section, Text};
use std::ptr;

pub struct GlGlyph {
    glyph_brush: GlyphBrush<[f32; 4 * 4], Extra, FontRef<'static>>,
    glyph_tex: GLuint,
    glyph_vertices: Vec<[f32; 4 * 4]>,
    glyph_indices: Vec<[u16; 6]>,
    text_program: GLuint,
}

impl GlGlyph {
    pub fn new() -> Self {
        let font = FontRef::try_from_slice(include_bytes!("../../../font/OpenSans-Regular.ttf")).unwrap();
        let glyph_brush = GlyphBrushBuilder::using_font(font).multithread(false).build();
        let (width, height) = glyph_brush.texture_dimensions();

        let glyph_tex = unsafe {
            let mut tex = 0;
            if cfg!(target_os = "linux") {
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            }
            gl::GenTextures(1, &mut tex);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            if cfg!(target_os = "linux") {
                gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RED as _, width as _, height as _, 0, gl::RED, gl::UNSIGNED_BYTE, ptr::null());
            } else {
                gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, width as _, height as _, 0, gl::RED, gl::UNSIGNED_BYTE, ptr::null());
            }
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            if cfg!(target_os = "linux") {
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 0);
            }
            tex
        };

        let text_program = unsafe {
            let vert_shader = create_shader("text", shader_source!("text_vert"), gl::VERTEX_SHADER).unwrap();
            let frag_shader = create_shader("text", shader_source!("text_frag"), gl::FRAGMENT_SHADER).unwrap();
            let program = create_program(&[vert_shader, frag_shader]).unwrap();
            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            gl::UseProgram(program);

            gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(program, "tex\0".as_ptr() as _), 0);

            gl::UseProgram(0);

            program
        };

        GlGlyph {
            glyph_brush,
            glyph_tex,
            glyph_vertices: Vec::new(),
            glyph_indices: Vec::new(),
            text_program,
        }
    }

    pub unsafe fn draw(&mut self, text: impl Into<String>) {
        self.glyph_brush.queue(Section::default().add_text(Text::new(&text.into()).with_scale(25.0)));
        let glyph_action;
        loop {
            glyph_action = self.glyph_brush.process_queued(
                |rect, tex| unsafe {
                    gl::BindTexture(gl::TEXTURE_2D, self.glyph_tex);
                    gl::TexSubImage2D(
                        gl::TEXTURE_2D,
                        0,
                        rect.min[0] as _,
                        rect.min[1] as _,
                        rect.width() as _,
                        rect.height() as _,
                        gl::RED,
                        gl::UNSIGNED_BYTE,
                        tex.as_ptr() as _,
                    );
                    gl::BindTexture(gl::TEXTURE_2D, 0);
                },
                |glyph| {
                    #[rustfmt::skip]
                    [
                        // Top left
                        glyph.pixel_coords.min.x,
                        glyph.pixel_coords.max.y,
                        glyph.tex_coords.min.x,
                        glyph.tex_coords.max.y,
                        // Top right
                        glyph.pixel_coords.max.x,
                        glyph.pixel_coords.max.y,
                        glyph.tex_coords.max.x,
                        glyph.tex_coords.max.y,
                        // Bottom right
                        glyph.pixel_coords.max.x,
                        glyph.pixel_coords.min.y,
                        glyph.tex_coords.max.x,
                        glyph.tex_coords.min.y,
                        // Bottom left
                        glyph.pixel_coords.min.x,
                        glyph.pixel_coords.min.y,
                        glyph.tex_coords.min.x,
                        glyph.tex_coords.min.y,
                    ]
                },
            );

            match glyph_action {
                Ok(_) => break,
                Err(BrushError::TextureTooSmall { /*suggested,*/ .. }) => {
                    todo!();
                    // self.glyph_brush.resize_texture(suggested.0, suggested.1);
                }
            }
        }

        match glyph_action.unwrap() {
            BrushAction::Draw(vertices) => {
                self.glyph_vertices = vertices;
                self.glyph_indices.clear();
                for i in 0..self.glyph_vertices.len() {
                    let n = i as u16 * 4;
                    self.glyph_indices.push([n, n + 1, n + 2, n, n + 2, n + 3]);
                }
            }
            BrushAction::ReDraw => {}
        }

        gl::UseProgram(self.text_program);

        gl::Enable(gl::BLEND);
        gl::BlendEquation(gl::FUNC_ADD);
        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, self.glyph_tex);

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, 0, self.glyph_vertices.as_ptr() as _);
        gl::DrawElements(gl::TRIANGLES, (6 * self.glyph_indices.len()) as _, gl::UNSIGNED_SHORT, self.glyph_indices.as_ptr() as _);

        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::Disable(gl::BLEND);
        gl::UseProgram(0);
    }
}

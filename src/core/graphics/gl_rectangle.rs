use crate::core::graphics::gl_utils::{create_program, create_shader, shader_source};
use gl::types::GLuint;
use std::ptr;

pub struct GlRectangle {
    rect_vertices: Vec<[f32; 8]>, // [x, y, r, g, b, a, unused1, unused2] for each vertex
    rect_indices: Vec<[u16; 6]>,
    rect_program: GLuint,
}

impl GlRectangle {
    pub fn new() -> Self {
        let rect_program = unsafe {
            let vert_shader = create_shader("rect", shader_source!("rect_vert"), gl::VERTEX_SHADER).unwrap();
            let frag_shader = create_shader("rect", shader_source!("rect_frag"), gl::FRAGMENT_SHADER).unwrap();
            let program = create_program(&[vert_shader, frag_shader]).unwrap();
            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            gl::UseProgram(program);

            gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);
            gl::BindAttribLocation(program, 1, "color\0".as_ptr() as _);

            gl::UseProgram(0);

            program
        };

        GlRectangle {
            rect_vertices: Vec::new(),
            rect_indices: Vec::new(),
            rect_program,
        }
    }

    // Helper method to render vertices
    unsafe fn render_vertices(&mut self) {
        gl::UseProgram(self.rect_program);

        gl::Enable(gl::BLEND);
        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

        gl::EnableVertexAttribArray(0);
        gl::EnableVertexAttribArray(1);
        
        // Position attribute (x, y)
        gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 8 * std::mem::size_of::<f32>() as i32, self.rect_vertices.as_ptr() as _);
        
        // Color attribute (r, g, b, a) - offset by 2 floats
        gl::VertexAttribPointer(1, 4, gl::FLOAT, gl::FALSE, 8 * std::mem::size_of::<f32>() as i32, 
            (self.rect_vertices.as_ptr() as *const u8).add(2 * std::mem::size_of::<f32>()) as _);

        gl::DrawElements(gl::TRIANGLES, (6 * self.rect_indices.len()) as i32, gl::UNSIGNED_SHORT, self.rect_indices.as_ptr() as _);

        gl::DisableVertexAttribArray(0);
        gl::DisableVertexAttribArray(1);
        gl::Disable(gl::BLEND);
        gl::UseProgram(0);
    }

    // Draw a filled rectangle
    pub unsafe fn draw_filled(&mut self, x: f32, y: f32, width: f32, height: f32, fill_color: (f32, f32, f32, f32)) {
        // Clear previous vertices and indices
        self.rect_vertices.clear();
        self.rect_indices.clear();

        // Create vertices for the filled rectangle (4 corners)
        let (r, g, b, a) = fill_color;
        let vertices = [
            // Top left
            [x, y + height, r, g, b, a, 0.0, 0.0],
            // Top right
            [x + width, y + height, r, g, b, a, 0.0, 0.0],
            // Bottom right
            [x + width, y, r, g, b, a, 0.0, 0.0],
            // Bottom left
            [x, y, r, g, b, a, 0.0, 0.0],
        ];

        self.rect_vertices.extend_from_slice(&vertices);

        // Create indices for two triangles forming the rectangle
        let indices = [0, 1, 2, 0, 2, 3]; // Two triangles: (0,1,2) and (0,2,3)
        self.rect_indices.push(indices);

        self.render_vertices();
    }

    // Draw a stroke-only rectangle (unfilled)
    pub unsafe fn draw_stroke(&mut self, x: f32, y: f32, width: f32, height: f32, stroke_color: (f32, f32, f32, f32), stroke_width: f32) {
        // Clear previous vertices and indices
        self.rect_vertices.clear();
        self.rect_indices.clear();

        let (r, g, b, a) = stroke_color;
        let half_stroke = stroke_width / 2.0;

        // Create vertices for 4 stroke rectangles (top, right, bottom, left)
        let vertices = [
            // Top stroke
            [x - half_stroke, y + height + half_stroke, r, g, b, a, 0.0, 0.0],
            [x + width + half_stroke, y + height + half_stroke, r, g, b, a, 0.0, 0.0],
            [x + width + half_stroke, y + height - half_stroke, r, g, b, a, 0.0, 0.0],
            [x - half_stroke, y + height - half_stroke, r, g, b, a, 0.0, 0.0],
            
            // Right stroke
            [x + width - half_stroke, y + height + half_stroke, r, g, b, a, 0.0, 0.0],
            [x + width + half_stroke, y + height + half_stroke, r, g, b, a, 0.0, 0.0],
            [x + width + half_stroke, y - half_stroke, r, g, b, a, 0.0, 0.0],
            [x + width - half_stroke, y - half_stroke, r, g, b, a, 0.0, 0.0],
            
            // Bottom stroke
            [x - half_stroke, y + half_stroke, r, g, b, a, 0.0, 0.0],
            [x + width + half_stroke, y + half_stroke, r, g, b, a, 0.0, 0.0],
            [x + width + half_stroke, y - half_stroke, r, g, b, a, 0.0, 0.0],
            [x - half_stroke, y - half_stroke, r, g, b, a, 0.0, 0.0],
            
            // Left stroke
            [x - half_stroke, y + height + half_stroke, r, g, b, a, 0.0, 0.0],
            [x + half_stroke, y + height + half_stroke, r, g, b, a, 0.0, 0.0],
            [x + half_stroke, y - half_stroke, r, g, b, a, 0.0, 0.0],
            [x - half_stroke, y - half_stroke, r, g, b, a, 0.0, 0.0],
        ];

        self.rect_vertices.extend_from_slice(&vertices);

        // Create indices for 4 rectangles (16 vertices = 4 rectangles * 4 vertices each)
        for i in 0..4 {
            let base = i * 4;
            let indices = [base, base + 1, base + 2, base, base + 2, base + 3];
            self.rect_indices.push(indices);
        }

        self.render_vertices();
    }

    // Draw a rectangle with both fill and stroke
    pub unsafe fn draw_filled_stroke(&mut self, x: f32, y: f32, width: f32, height: f32, fill_color: (f32, f32, f32, f32), stroke_color: (f32, f32, f32, f32), stroke_width: f32) {
        // First draw the fill
        self.draw_filled(x, y, width, height, fill_color);
        // Then draw the stroke
        self.draw_stroke(x, y, width, height, stroke_color, stroke_width);
    }
}

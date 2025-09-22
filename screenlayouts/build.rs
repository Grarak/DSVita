use nalgebra::{Matrix3, Vector2};
use std::env;
use std::f32::consts::PI;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

pub const HOST_SCREEN_WIDTH: usize = 960;
pub const HOST_SCREEN_HEIGHT: usize = 544;

pub const GUEST_SCREEN_WIDTH: usize = 256;
pub const GUEST_SCREEN_HEIGHT: usize = 192;

pub fn get_screen_layouts() -> Vec<[[f32; 9]; 4]> {
    let mut layouts = Vec::new();

    {
        let guest_width = HOST_SCREEN_WIDTH as f32 / 2.0;
        let width_scale = guest_width / GUEST_SCREEN_WIDTH as f32;
        let guest_height = GUEST_SCREEN_HEIGHT as f32 * width_scale;
        let height_remaining_space = HOST_SCREEN_HEIGHT as f32 - guest_height;
        let mtx = Matrix3::new_translation(&Vector2::new(0.0, height_remaining_space / 2.0))
            * Matrix3::new_translation(&Vector2::new(guest_width / 2.0, guest_height / 2.0))
            * Matrix3::new_scaling(width_scale);
        let b_trans = Matrix3::new_translation(&Vector2::new(guest_width, 0.0));

        layouts.push([mtx, b_trans * mtx]);
    }

    {
        let half_width = HOST_SCREEN_WIDTH as f32 / 2.0;
        let full_height_scale = HOST_SCREEN_HEIGHT as f32 / GUEST_SCREEN_WIDTH as f32;
        let guest_height = GUEST_SCREEN_HEIGHT as f32 * full_height_scale;
        let half_width_space = half_width - guest_height;
        let mtx = Matrix3::new_translation(&Vector2::new(guest_height / 2.0 + half_width_space, HOST_SCREEN_HEIGHT as f32 / 2.0))
            * Matrix3::new_rotation(PI + PI / 2.0)
            * Matrix3::new_scaling(full_height_scale);
        let b_trans = Matrix3::new_translation(&Vector2::new(guest_height, 0.0));

        layouts.push([mtx, b_trans * mtx]);
    }

    {
        let full_height_scale = HOST_SCREEN_HEIGHT as f32 / GUEST_SCREEN_HEIGHT as f32;
        let guest_top_width = GUEST_SCREEN_WIDTH as f32 * full_height_scale;
        let width_remaining_space = HOST_SCREEN_WIDTH as f32 - guest_top_width;
        let guest_bottom_scale = width_remaining_space / GUEST_SCREEN_WIDTH as f32;
        let guest_bottom_height = GUEST_SCREEN_HEIGHT as f32 * guest_bottom_scale;
        let height_remaining_space = HOST_SCREEN_HEIGHT as f32 - guest_bottom_height;
        let top_mtx = Matrix3::new_translation(&Vector2::new(guest_top_width / 2.0, HOST_SCREEN_HEIGHT as f32 / 2.0)) * Matrix3::new_scaling(full_height_scale);
        let bottom_mtx =
            Matrix3::new_translation(&Vector2::new(width_remaining_space / 2.0 + guest_top_width, guest_bottom_height / 2.0 + height_remaining_space / 2.0)) * Matrix3::new_scaling(guest_bottom_scale);

        layouts.push([top_mtx, bottom_mtx]);
    }

    {
        let full_height_scale = HOST_SCREEN_HEIGHT as f32 / GUEST_SCREEN_HEIGHT as f32;
        let guest_top_width = GUEST_SCREEN_WIDTH as f32 * full_height_scale;
        let width_remaining_space = HOST_SCREEN_WIDTH as f32 - guest_top_width;
        let top_mtx = Matrix3::new_translation(&Vector2::new(guest_top_width / 2.0 + width_remaining_space / 2.0, HOST_SCREEN_HEIGHT as f32 / 2.0)) * Matrix3::new_scaling(full_height_scale);

        layouts.push([top_mtx, Matrix3::zeros()]);
    }

    layouts
        .iter()
        .map(|mtxs| {
            let flatten = |mtx: &Matrix3<f32>| [mtx[0], mtx[1], mtx[2], mtx[3], mtx[4], mtx[5], mtx[6], mtx[7], mtx[8]];
            [
                flatten(&mtxs[0]),
                flatten(&mtxs[1]),
                flatten(&mtxs[0].try_inverse().unwrap_or(Matrix3::zeros())),
                flatten(&mtxs[1].try_inverse().unwrap_or(Matrix3::zeros())),
            ]
        })
        .collect()
}

pub fn main() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let screen_layouts_file = out_path.join("screen_layouts.rs");
    let layouts = get_screen_layouts();
    let mut code = format!("pub const SCREEN_LAYOUTS: [[[f32; 9]; 4]; {}] = [\n", layouts.len());
    for layout in layouts {
        code += "\t[\n";
        code += &format!("\t\t{:?},\n", layout[0]);
        code += &format!("\t\t{:?},\n", layout[1]);
        code += &format!("\t\t{:?},\n", layout[2]);
        code += &format!("\t\t{:?},\n", layout[3]);
        code += "\t],\n";
    }
    code += "];\n";
    File::create(screen_layouts_file).unwrap().write_all(code.as_bytes()).unwrap();
}

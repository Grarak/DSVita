use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::math;
use crate::presenter::{PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::settings::SettingValue;
use ini::{Properties, SectionSetter};
use nalgebra::{Matrix3, Vector2, Vector3};
use std::f32::consts::PI;
use std::ffi::CString;
use std::str::FromStr;

fn get_predefined_layouts() -> Vec<(&'static str, [[f32; 9]; 4])> {
    let mut layouts = Vec::new();

    {
        let guest_width = PRESENTER_SCREEN_WIDTH as f32 / 2.0;
        let width_scale = guest_width / DISPLAY_WIDTH as f32;
        let guest_height = DISPLAY_HEIGHT as f32 * width_scale;
        let height_remaining_space = PRESENTER_SCREEN_HEIGHT as f32 - guest_height;
        let mtx = Matrix3::new_translation(&Vector2::new(0.0, height_remaining_space / 2.0))
            * Matrix3::new_translation(&Vector2::new(guest_width / 2.0, guest_height / 2.0))
            * Matrix3::new_scaling(width_scale);
        let b_trans = Matrix3::new_translation(&Vector2::new(guest_width, 0.0));

        layouts.push(("Side by side", [mtx, b_trans * mtx]));
    }

    {
        let half_width = PRESENTER_SCREEN_WIDTH as f32 / 2.0;
        let full_height_scale = PRESENTER_SCREEN_HEIGHT as f32 / DISPLAY_WIDTH as f32;
        let guest_height = DISPLAY_HEIGHT as f32 * full_height_scale;
        let half_width_space = half_width - guest_height;
        let mtx = Matrix3::new_translation(&Vector2::new(guest_height / 2.0 + half_width_space, PRESENTER_SCREEN_HEIGHT as f32 / 2.0))
            * Matrix3::new_rotation(PI + PI / 2.0)
            * Matrix3::new_scaling(full_height_scale);
        let b_trans = Matrix3::new_translation(&Vector2::new(guest_height, 0.0));

        layouts.push(("Rotate", [mtx, b_trans * mtx]));
    }

    {
        let full_height_scale = PRESENTER_SCREEN_HEIGHT as f32 / DISPLAY_HEIGHT as f32;
        let guest_top_width = DISPLAY_WIDTH as f32 * full_height_scale;
        let width_remaining_space = PRESENTER_SCREEN_WIDTH as f32 - guest_top_width;
        let top_mtx = Matrix3::new_translation(&Vector2::new(guest_top_width / 2.0 + width_remaining_space / 2.0, PRESENTER_SCREEN_HEIGHT as f32 / 2.0)) * Matrix3::new_scaling(full_height_scale);

        layouts.push((
            "Single",
            [top_mtx, Matrix3::new_translation(&Vector2::new(-(PRESENTER_SCREEN_WIDTH as f32), -(PRESENTER_SCREEN_HEIGHT as f32)))],
        ));
    }

    {
        let full_height_scale = PRESENTER_SCREEN_HEIGHT as f32 / DISPLAY_HEIGHT as f32;
        let guest_top_width = DISPLAY_WIDTH as f32 * full_height_scale;
        let width_remaining_space = PRESENTER_SCREEN_WIDTH as f32 - guest_top_width;
        let guest_bottom_scale = width_remaining_space / DISPLAY_WIDTH as f32;
        let guest_bottom_height = DISPLAY_HEIGHT as f32 * guest_bottom_scale;
        let height_remaining_space = PRESENTER_SCREEN_HEIGHT as f32 - guest_bottom_height;
        let top_mtx = Matrix3::new_translation(&Vector2::new(guest_top_width / 2.0, PRESENTER_SCREEN_HEIGHT as f32 / 2.0)) * Matrix3::new_scaling(full_height_scale);
        let bottom_mtx =
            Matrix3::new_translation(&Vector2::new(width_remaining_space / 2.0 + guest_top_width, guest_bottom_height / 2.0 + height_remaining_space / 2.0)) * Matrix3::new_scaling(guest_bottom_scale);

        layouts.push(("Focus", [top_mtx, bottom_mtx]));
    }

    {
        let full_height_scale = PRESENTER_SCREEN_HEIGHT as f32 / DISPLAY_HEIGHT as f32;
        let guest_top_width = DISPLAY_WIDTH as f32 * full_height_scale;
        let width_remaining_space = PRESENTER_SCREEN_WIDTH as f32 - guest_top_width;
        let top_mtx = Matrix3::new_translation(&Vector2::new(guest_top_width / 2.0 + width_remaining_space / 2.0, PRESENTER_SCREEN_HEIGHT as f32 / 2.0)) * Matrix3::new_scaling(full_height_scale);
        let bottom_mtx = Matrix3::new_translation(&Vector2::new(
            (PRESENTER_SCREEN_WIDTH as usize - DISPLAY_WIDTH) as f32,
            (PRESENTER_SCREEN_HEIGHT as usize - DISPLAY_HEIGHT) as f32,
        )) * Matrix3::new_translation(&Vector2::new(DISPLAY_WIDTH as f32 / 2.0, DISPLAY_HEIGHT as f32 / 2.0));

        layouts.push(("Focus Overlap", [top_mtx, bottom_mtx]));
    }

    {
        let full_height_scale = PRESENTER_SCREEN_HEIGHT as f32 / DISPLAY_HEIGHT as f32;
        let full_width_scale = PRESENTER_SCREEN_WIDTH as f32 / DISPLAY_WIDTH as f32;
        let guest_top_width = DISPLAY_WIDTH as f32 * full_width_scale;
        let guest_top_height = DISPLAY_HEIGHT as f32 * full_height_scale;
        let top_mtx =
            Matrix3::new_translation(&Vector2::new(guest_top_width / 2.0, guest_top_height as f32 / 2.0)) * Matrix3::new_nonuniform_scaling(&Vector2::new(full_width_scale, full_height_scale));
        let bottom_mtx = Matrix3::new_translation(&Vector2::new(
            (PRESENTER_SCREEN_WIDTH as usize - DISPLAY_WIDTH) as f32,
            (PRESENTER_SCREEN_HEIGHT as usize - DISPLAY_HEIGHT) as f32,
        )) * Matrix3::new_translation(&Vector2::new(DISPLAY_WIDTH as f32 / 2.0, DISPLAY_HEIGHT as f32 / 2.0));

        layouts.push(("Stretch Overlap", [top_mtx, bottom_mtx]));
    }

    {
        let half_height = PRESENTER_SCREEN_HEIGHT as f32 / 2.0;
        let half_width = PRESENTER_SCREEN_WIDTH as f32 / 2.0;
        let half_height_scale = half_height / DISPLAY_HEIGHT as f32;
        let mtx = Matrix3::new_translation(&Vector2::new(half_width, half_height / 2.0)) * Matrix3::new_scaling(half_height_scale);

        layouts.push(("Vertical", [mtx, Matrix3::new_translation(&Vector2::new(0.0, half_height)) * mtx]));
    }

    {
        let half_width = PRESENTER_SCREEN_WIDTH as f32 / 2.0;
        let full_height_scale = (PRESENTER_SCREEN_HEIGHT as f32 / DISPLAY_WIDTH as f32).floor();
        let guest_width = DISPLAY_WIDTH as f32 * full_height_scale;
        let guest_height = DISPLAY_HEIGHT as f32 * full_height_scale;
        let half_width_remaining_space = half_width - guest_height;
        let height_remaining_space = PRESENTER_SCREEN_HEIGHT as f32 - guest_width;
        let mtx = Matrix3::new_translation(&Vector2::new(guest_height / 2.0 + half_width_remaining_space, guest_width / 2.0 + height_remaining_space / 2.0))
            * Matrix3::new_rotation(PI + PI / 2.0)
            * Matrix3::new_scaling(full_height_scale);
        let b_trans = Matrix3::new_translation(&Vector2::new(guest_height, 0.0));

        layouts.push(("Int Rotate", [mtx, b_trans * mtx]));
    }

    {
        let full_height_scale = PRESENTER_SCREEN_HEIGHT as f32 / DISPLAY_HEIGHT as f32;
        let width_scale = 3.0;
        let guest_top_width = DISPLAY_WIDTH as f32 * width_scale;
        let guest_top_height = DISPLAY_HEIGHT as f32 * full_height_scale;
        let width_remaining_space = PRESENTER_SCREEN_WIDTH as f32 - guest_top_width;
        let height_remaining_space = PRESENTER_SCREEN_HEIGHT as f32 - guest_top_height;
        let top_mtx = Matrix3::new_translation(&Vector2::new(
            guest_top_width / 2.0 + width_remaining_space / 2.0,
            guest_top_height / 2.0 + height_remaining_space / 2.0,
        )) * Matrix3::new_nonuniform_scaling(&Vector2::new(width_scale, full_height_scale));
        let bottom_mtx = Matrix3::new_translation(&Vector2::new(
            (PRESENTER_SCREEN_WIDTH as usize - DISPLAY_WIDTH) as f32,
            (PRESENTER_SCREEN_HEIGHT as usize - DISPLAY_HEIGHT) as f32,
        )) * Matrix3::new_translation(&Vector2::new(DISPLAY_WIDTH as f32 / 2.0, DISPLAY_HEIGHT as f32 / 2.0));

        layouts.push(("Int Focus Overlap", [top_mtx, bottom_mtx]));
    }

    layouts
        .iter()
        .map(|(name, mtxs)| {
            let flatten = |mtx: &Matrix3<f32>| [mtx[0], mtx[1], mtx[2], mtx[3], mtx[4], mtx[5], mtx[6], mtx[7], mtx[8]];
            (
                *name,
                [flatten(&mtxs[0]), flatten(&mtxs[1]), flatten(&mtxs[0].try_inverse().unwrap()), flatten(&mtxs[1].try_inverse().unwrap())],
            )
        })
        .collect()
}

pub struct ScreenLayouts {
    predefined: Vec<(&'static str, [[f32; 9]; 4])>,
    custom: Vec<CustomLayout>,
}

impl ScreenLayouts {
    pub fn new() -> Self {
        ScreenLayouts {
            predefined: get_predefined_layouts(),
            custom: Vec::new(),
        }
    }

    pub fn populate_custom_layouts(&mut self, custom_layouts: &[CustomLayout]) {
        self.custom = custom_layouts.to_vec();
    }

    fn convert_custom_layout(&self, index: usize) -> [[f32; 9]; 4] {
        let layout = &self.custom[index];

        let create_mtx = |index: usize| {
            let width_scale = layout.sizes[index].0 as f32 / DISPLAY_WIDTH as f32;
            let height_scale = layout.sizes[index].1 as f32 / DISPLAY_HEIGHT as f32;
            let rot = layout.rotation[index] as f32;
            let pos = Vector3::new(-(DISPLAY_WIDTH as f32 / 2.0), -(DISPLAY_HEIGHT as f32 / 2.0), 1.0);
            let mtx = Matrix3::new_rotation(PI / 180.0 * rot) * Matrix3::new_nonuniform_scaling(&Vector2::new(width_scale, height_scale));
            let pos = mtx * pos;
            let x_trans = layout.pos[index].0 as f32;
            let y_trans = layout.pos[index].1 as f32;
            Matrix3::new_translation(&Vector2::new(x_trans, y_trans)) * Matrix3::new_translation(&Vector2::new(pos.x.abs(), pos.y.abs())) * mtx
        };

        let mtx_top = create_mtx(0);
        let mtx_bottom = create_mtx(1);

        let flatten = |mtx: &Matrix3<f32>| [mtx[0], mtx[1], mtx[2], mtx[3], mtx[4], mtx[5], mtx[6], mtx[7], mtx[8]];
        [
            flatten(&mtx_top),
            flatten(&mtx_bottom),
            flatten(&mtx_top.try_inverse().unwrap_or_default()),
            flatten(&mtx_bottom.try_inverse().unwrap_or_default()),
        ]
    }

    pub fn len(&self) -> usize {
        self.predefined.len() + self.custom.len()
    }

    pub fn get_name(&self, index: usize) -> &str {
        if index < self.predefined.len() {
            self.predefined[index].0
        } else {
            let index = index - self.predefined.len();
            &self.custom[index].name
        }
    }

    pub fn get(&self, index: usize) -> [[f32; 9]; 4] {
        if index < self.predefined.len() {
            self.predefined[index].1
        } else {
            let index = index - self.predefined.len();
            self.convert_custom_layout(index)
        }
    }
}

const SCALE_FACTORS: [f32; 8] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

pub struct ScreenLayout {
    pub index: usize,
    pub swap: bool,
    top_scale_index: usize,
    bottom_scale_index: usize,
    overlap: bool,
    screen_top: [f32; 16],
    screen_bottom: [f32; 16],
    bottom_inverse_mtx: [f32; 9],
}

impl ScreenLayout {
    pub fn scale_settings_value() -> SettingValue {
        SettingValue::List(3, SCALE_FACTORS.iter().map(|factor| format!("{}%", (factor * 100.0) as u8)).collect())
    }

    fn scale_matrix(factor: f32) -> [f32; 9] {
        [factor, 0.0, 0.0, 0.0, factor, 0.0, 0.0, 0.0, 1.0]
    }

    pub fn new(screen_layouts: &ScreenLayouts, mut index: usize, swap: bool, top_scale_index: usize, bottom_scale_index: usize) -> Self {
        const GUEST_DISPLAY_DIM_MTX: [[f32; 3]; 4] = [
            [-(DISPLAY_WIDTH as f32 / 2.0), -(DISPLAY_HEIGHT as f32 / 2.0), 1.0],
            [DISPLAY_WIDTH as f32 / 2.0, -(DISPLAY_HEIGHT as f32 / 2.0), 1.0],
            [DISPLAY_WIDTH as f32 / 2.0, DISPLAY_HEIGHT as f32 / 2.0, 1.0],
            [-(DISPLAY_WIDTH as f32 / 2.0), DISPLAY_HEIGHT as f32 / 2.0, 1.0],
        ];
        if index >= screen_layouts.len() {
            index = 0;
        }
        let mtxs = screen_layouts.get(index);
        let mut a_mtx = mtxs[0];
        let mut b_mtx = mtxs[1];

        let top_scale_mtx = Self::scale_matrix(SCALE_FACTORS[top_scale_index]);
        let bottom_scale_mtx = Self::scale_matrix(SCALE_FACTORS[bottom_scale_index]);

        unsafe {
            math::neon::matmul3_neon(a_mtx.as_ptr() as _, top_scale_mtx.as_ptr() as _, a_mtx.as_mut_ptr());
            math::neon::matmul3_neon(b_mtx.as_ptr() as _, bottom_scale_mtx.as_ptr() as _, b_mtx.as_mut_ptr());
        }

        let mut screen_top = [[0.0; 3]; 4];
        let mut screen_bottom = [[0.0; 3]; 4];
        let mut bottom_inverse_mtx = mtxs[if swap { 2 } else { 3 }];

        unsafe {
            let inverse_scale_mtx = Self::scale_matrix(1.0 / SCALE_FACTORS[if swap { top_scale_index } else { bottom_scale_index }]);
            math::neon::matmul3_neon(inverse_scale_mtx.as_ptr() as _, bottom_inverse_mtx.as_ptr() as _, bottom_inverse_mtx.as_mut_ptr());
        }

        let overlap = unsafe {
            for i in 0..GUEST_DISPLAY_DIM_MTX.len() {
                math::neon::matvec3_neon(a_mtx.as_ptr() as _, GUEST_DISPLAY_DIM_MTX[i].as_ptr() as _, screen_top[i].as_mut_ptr());
                math::neon::matvec3_neon(b_mtx.as_ptr() as _, GUEST_DISPLAY_DIM_MTX[i].as_ptr() as _, screen_bottom[i].as_mut_ptr());
            }

            let overlap = ((screen_bottom[0][0].round() > screen_top[0][0].round() && screen_bottom[0][0].round() < screen_top[1][0].round())
                || (screen_bottom[1][0].round() > screen_top[0][0].round() && screen_bottom[1][0].round() < screen_top[1][0].round())
                || (screen_bottom[2][0].round() > screen_top[0][0] && screen_bottom[2][0].round() < screen_top[1][0].round())
                || (screen_bottom[3][0].round() > screen_top[0][0] && screen_bottom[0][0].round() < screen_top[3][0].round()))
                && ((screen_bottom[0][1].round() > screen_top[0][1].round() && screen_bottom[0][1].round() < screen_top[1][1].round())
                    || (screen_bottom[1][1].round() > screen_top[0][1].round() && screen_bottom[1][1].round() < screen_top[1][1].round())
                    || (screen_bottom[2][1].round() > screen_top[0][1].round() && screen_bottom[2][1].round() < screen_top[1][1].round())
                    || (screen_bottom[3][1].round() > screen_top[0][1].round() && screen_bottom[0][1].round() < screen_top[3][1].round()));

            for i in 0..GUEST_DISPLAY_DIM_MTX.len() {
                screen_top[i][0] = screen_top[i][0] / PRESENTER_SCREEN_WIDTH as f32 * 2.0 - 1.0;
                screen_top[i][1] = 1.0 - screen_top[i][1] / PRESENTER_SCREEN_HEIGHT as f32 * 2.0;
                screen_bottom[i][0] = screen_bottom[i][0] / PRESENTER_SCREEN_WIDTH as f32 * 2.0 - 1.0;
                screen_bottom[i][1] = 1.0 - screen_bottom[i][1] / PRESENTER_SCREEN_HEIGHT as f32 * 2.0;
            }

            overlap
        };

        ScreenLayout {
            index,
            swap,
            top_scale_index,
            bottom_scale_index,
            overlap,
            screen_top: [
                screen_top[0][0],
                screen_top[0][1],
                0.0,
                1.0,
                screen_top[1][0],
                screen_top[1][1],
                1.0,
                1.0,
                screen_top[2][0],
                screen_top[2][1],
                1.0,
                0.0,
                screen_top[3][0],
                screen_top[3][1],
                0.0,
                0.0,
            ],
            screen_bottom: [
                screen_bottom[0][0],
                screen_bottom[0][1],
                0.0,
                1.0,
                screen_bottom[1][0],
                screen_bottom[1][1],
                1.0,
                1.0,
                screen_bottom[2][0],
                screen_bottom[2][1],
                1.0,
                0.0,
                screen_bottom[3][0],
                screen_bottom[3][1],
                0.0,
                0.0,
            ],
            bottom_inverse_mtx,
        }
    }

    pub fn apply_settings_event(&self, screen_layouts: &ScreenLayouts, offset: i8, swap: bool, top_screen_scale_offset: i8, bottom_screen_scale_offset: i8) -> ScreenLayout {
        ScreenLayout::new(
            screen_layouts,
            ((screen_layouts.len() as isize + (self.index as isize + offset as isize)) % screen_layouts.len() as isize) as usize,
            self.swap ^ swap,
            ((SCALE_FACTORS.len() as isize + (self.top_scale_index as isize + top_screen_scale_offset as isize)) % SCALE_FACTORS.len() as isize) as usize,
            ((SCALE_FACTORS.len() as isize + (self.bottom_scale_index as isize + bottom_screen_scale_offset as isize)) % SCALE_FACTORS.len() as isize) as usize,
        )
    }

    pub fn normalize_touch_points(&self, x: i16, y: i16) -> (i16, i16) {
        let mut touch_points = [x as f32, y as f32, 1.0];
        unsafe { math::neon::matvec3_neon(self.bottom_inverse_mtx.as_ptr() as _, touch_points.as_ptr() as _, touch_points.as_mut_ptr() as _) };
        (touch_points[0] as i16 + DISPLAY_WIDTH as i16 / 2, touch_points[1] as i16 + DISPLAY_HEIGHT as i16 / 2)
    }

    pub fn get_screen_top(&self) -> (&[f32; 16], f32) {
        if self.swap {
            (&self.screen_bottom, if self.overlap { 0.5 } else { 1.0 })
        } else {
            (&self.screen_top, 1.0)
        }
    }

    pub fn get_screen_bottom(&self) -> (&[f32; 16], f32) {
        if self.swap {
            (&self.screen_top, 1.0)
        } else {
            (&self.screen_bottom, if self.overlap { 0.5 } else { 1.0 })
        }
    }
}

#[derive(Clone)]
pub struct CustomLayout {
    pub name: String,
    pub sizes: [(u16, u16); 2],
    pub pos: [(u16, u16); 2],
    pub rotation: [u16; 2],
}

impl CustomLayout {
    pub fn name_c_str(&self) -> CString {
        CString::from_str(&self.name).unwrap()
    }

    pub fn width_str(&self, index: usize) -> String {
        self.sizes[index].0.to_string()
    }

    pub fn width_c_str(&self, index: usize) -> CString {
        CString::from_str(&self.width_str(index)).unwrap()
    }

    pub fn set_width(&mut self, index: usize, width: &str) -> bool {
        match width.parse::<u16>() {
            Ok(value) => {
                self.sizes[index].0 = value;
                true
            }
            Err(_) => false,
        }
    }

    pub fn height_str(&self, index: usize) -> String {
        self.sizes[index].1.to_string()
    }

    pub fn height_c_str(&self, index: usize) -> CString {
        CString::from_str(&self.height_str(index)).unwrap()
    }

    pub fn set_height(&mut self, index: usize, height: &str) -> bool {
        match height.parse::<u16>() {
            Ok(value) => {
                self.sizes[index].1 = value;
                true
            }
            Err(_) => false,
        }
    }

    pub fn pos_x_str(&self, index: usize) -> String {
        self.pos[index].0.to_string()
    }

    pub fn pos_x_c_str(&self, index: usize) -> CString {
        CString::from_str(&self.pos_x_str(index)).unwrap()
    }

    pub fn set_pos_x(&mut self, index: usize, pos_x: &str) -> bool {
        match pos_x.parse::<u16>() {
            Ok(value) => {
                self.pos[index].0 = value;
                true
            }
            Err(_) => false,
        }
    }

    pub fn pos_y_str(&self, index: usize) -> String {
        self.pos[index].1.to_string()
    }

    pub fn pos_y_c_str(&self, index: usize) -> CString {
        CString::from_str(&self.pos_y_str(index)).unwrap()
    }

    pub fn set_pos_y(&mut self, index: usize, pos_y: &str) -> bool {
        match pos_y.parse::<u16>() {
            Ok(value) => {
                self.pos[index].1 = value;
                true
            }
            Err(_) => false,
        }
    }

    pub fn rot_str(&self, index: usize) -> String {
        self.rotation[index].to_string()
    }

    pub fn rot_c_str(&self, index: usize) -> CString {
        CString::from_str(&self.rot_str(index)).unwrap()
    }

    pub fn set_rot(&mut self, index: usize, rot: &str) -> bool {
        match rot.parse::<u16>() {
            Ok(value) => {
                self.rotation[index] = value;
                true
            }
            Err(_) => false,
        }
    }

    fn parse_ini_tuple(value: Option<&str>) -> (u16, u16) {
        match value {
            None => (0, 0),
            Some(value) => {
                let values: Vec<&str> = value.split(",").collect();
                if values.len() == 2 {
                    (values[0].parse::<u16>().unwrap_or(0), values[1].parse::<u16>().unwrap_or(0))
                } else {
                    (0, 0)
                }
            }
        }
    }

    pub fn from_ini(name: &str, props: &Properties) -> Self {
        CustomLayout {
            name: name.to_string(),
            sizes: [Self::parse_ini_tuple(props.get("sizes_top")), Self::parse_ini_tuple(props.get("sizes_bottom"))],
            pos: [Self::parse_ini_tuple(props.get("pos_top")), Self::parse_ini_tuple(props.get("pos_bottom"))],
            rotation: [
                props.get("rot_top").unwrap_or("0").parse::<u16>().unwrap_or(0),
                props.get("rot_bottom").unwrap_or("0").parse::<u16>().unwrap_or(0),
            ],
        }
    }

    fn ini_tuple(value: (u16, u16)) -> String {
        format!("{},{}", value.0, value.1)
    }

    pub fn to_ini(&self, section_setter: &mut SectionSetter) {
        section_setter.set("sizes_top", Self::ini_tuple(self.sizes[0]));
        section_setter.set("sizes_bottom", Self::ini_tuple(self.sizes[1]));
        section_setter.set("pos_top", Self::ini_tuple(self.pos[0]));
        section_setter.set("pos_bottom", Self::ini_tuple(self.pos[1]));
        section_setter.set("rot_top", self.rotation[0].to_string());
        section_setter.set("rot_bottom", self.rotation[1].to_string());
    }
}

impl Default for CustomLayout {
    fn default() -> Self {
        CustomLayout {
            name: "".to_string(),
            sizes: [(DISPLAY_WIDTH as u16, DISPLAY_HEIGHT as u16); 2],
            pos: [(0, 0), (DISPLAY_WIDTH as u16, 0)],
            rotation: [0; 2],
        }
    }
}

use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::math;
use crate::presenter::{PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::settings::SettingValue;
use screenlayouts::SCREEN_LAYOUTS;

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
    pub fn settings_value() -> SettingValue {
        SettingValue::List(0, SCREEN_LAYOUTS.iter().map(|(name, _)| name.to_string()).collect())
    }

    pub fn scale_settings_value() -> SettingValue {
        SettingValue::List(3, SCALE_FACTORS.iter().map(|factor| format!("{}%", (factor * 100.0) as u8)).collect())
    }

    fn scale_matrix(factor: f32) -> [f32; 9] {
        [factor, 0.0, 0.0, 0.0, factor, 0.0, 0.0, 0.0, 1.0]
    }

    pub fn new(index: usize, swap: bool, top_scale_index: usize, bottom_scale_index: usize) -> Self {
        const GUEST_DISPLAY_DIM_MTX: [[f32; 3]; 4] = [
            [-(DISPLAY_WIDTH as f32 / 2.0), -(DISPLAY_HEIGHT as f32 / 2.0), 1.0],
            [DISPLAY_WIDTH as f32 / 2.0, -(DISPLAY_HEIGHT as f32 / 2.0), 1.0],
            [DISPLAY_WIDTH as f32 / 2.0, DISPLAY_HEIGHT as f32 / 2.0, 1.0],
            [-(DISPLAY_WIDTH as f32 / 2.0), DISPLAY_HEIGHT as f32 / 2.0, 1.0],
        ];
        let mut a_mtx = SCREEN_LAYOUTS[index].1[0];
        let mut b_mtx = SCREEN_LAYOUTS[index].1[1];

        let top_scale_mtx = Self::scale_matrix(SCALE_FACTORS[top_scale_index]);
        let bottom_scale_mtx = Self::scale_matrix(SCALE_FACTORS[bottom_scale_index]);

        unsafe {
            math::neon::matmul3_neon(a_mtx.as_ptr() as _, top_scale_mtx.as_ptr() as _, a_mtx.as_mut_ptr());
            math::neon::matmul3_neon(b_mtx.as_ptr() as _, bottom_scale_mtx.as_ptr() as _, b_mtx.as_mut_ptr());
        }

        let mut screen_top = [[0.0; 3]; 4];
        let mut screen_bottom = [[0.0; 3]; 4];
        let mut bottom_inverse_mtx = SCREEN_LAYOUTS[index].1[if swap { 2 } else { 3 }];

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

    pub fn apply_settings_event(&self, offset: i8, swap: bool, top_screen_scale_offset: i8, bottom_screen_scale_offset: i8) -> ScreenLayout {
        ScreenLayout::new(
            ((SCREEN_LAYOUTS.len() as isize + (self.index as isize + offset as isize)) % SCREEN_LAYOUTS.len() as isize) as usize,
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

use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::math;
use crate::presenter::{PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::settings::SettingValue;
use screenlayouts::SCREEN_LAYOUTS;

pub struct ScreenLayout {
    pub index: usize,
    pub swap: bool,
    overlap: bool,
    screen_top: [f32; 16],
    screen_bottom: [f32; 16],
}

impl ScreenLayout {
    pub fn settings_value() -> SettingValue {
        SettingValue::List(0, (0..SCREEN_LAYOUTS.len()).map(|i| i.to_string()).collect())
    }

    pub fn new(index: usize, swap: bool) -> Self {
        const GUEST_DISPLAY_DIM_MTX: [[f32; 3]; 4] = [
            [-(DISPLAY_WIDTH as f32 / 2.0), -(DISPLAY_HEIGHT as f32 / 2.0), 1.0],
            [DISPLAY_WIDTH as f32 / 2.0, -(DISPLAY_HEIGHT as f32 / 2.0), 1.0],
            [DISPLAY_WIDTH as f32 / 2.0, DISPLAY_HEIGHT as f32 / 2.0, 1.0],
            [-(DISPLAY_WIDTH as f32 / 2.0), DISPLAY_HEIGHT as f32 / 2.0, 1.0],
        ];
        let a_mtx = &SCREEN_LAYOUTS[index][0];
        let b_mtx = &SCREEN_LAYOUTS[index][1];
        let mut screen_top = [[0.0; 3]; 4];
        let mut screen_bottom = [[0.0; 3]; 4];
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
        }
    }

    pub fn apply_settings_event(&self, offset: i8, swap: bool) -> ScreenLayout {
        ScreenLayout::new(
            ((SCREEN_LAYOUTS.len() as isize + (self.index as isize + offset as isize)) % SCREEN_LAYOUTS.len() as isize) as usize,
            self.swap ^ swap,
        )
    }

    pub fn get_bottom_inverse_mtx(&self) -> &[f32; 9] {
        &SCREEN_LAYOUTS[self.index][if self.swap { 2 } else { 3 }]
    }

    pub fn normalize_touch_points(&self, x: i16, y: i16) -> (i16, i16) {
        let mut touch_points = [x as f32, y as f32, 1.0];
        unsafe { math::neon::matvec3_neon(self.get_bottom_inverse_mtx().as_ptr() as _, touch_points.as_ptr() as _, touch_points.as_mut_ptr() as _) };
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

use crate::hle::gpu::gpu::{DISPLAY_HEIGHT, DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH};
use crate::hle::input::Keycode;
use crate::presenter::{PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use vitasdk_sys::*;

type Vita2dPgf = *mut c_void;

#[repr(C)]
pub struct Vita2dTexture {
    gxm_tex: SceGxmTexture,
    data_uid: SceUID,
    palette_uid: SceUID,
    gxm_rtgt: *mut SceGxmRenderTarget,
    gxm_sfc: SceGxmColorSurface,
    gxm_sfd: SceGxmDepthStencilSurface,
    depth_uid: SceUID,
}

#[link(name = "vita2d", kind = "static")]
extern "C" {
    pub fn vita2d_init();
    pub fn vita2d_fini() -> c_int;

    pub fn vita2d_clear_screen();
    pub fn vita2d_swap_buffers();

    pub fn vita2d_start_drawing();
    pub fn vita2d_end_drawing();

    pub fn vita2d_set_clear_color(color: c_uint);

    pub fn vita2d_create_empty_texture(w: c_uint, h: c_uint) -> *mut Vita2dTexture;

    pub fn vita2d_free_texture(texture: *mut Vita2dTexture);

    pub fn vita2d_texture_get_datap(texture: *const Vita2dTexture) -> *mut c_void;

    pub fn vita2d_draw_texture_part_scale(
        texture: *const Vita2dTexture,
        x: c_float,
        y: c_float,
        tex_x: c_float,
        tex_y: c_float,
        tex_w: c_float,
        tex_h: c_float,
        x_scale: c_float,
        y_scale: c_float,
    );

    pub fn vita2d_load_default_pgf() -> *mut Vita2dPgf;
    pub fn vita2d_free_pgf(font: *mut Vita2dPgf);
    pub fn vita2d_pgf_draw_text(
        font: *mut Vita2dPgf,
        x: c_int,
        y: c_int,
        color: c_uint,
        scale: c_float,
        text: *const c_char,
    ) -> c_int;
}

const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32)
}

const KEY_CODE_MAPPING: [(SceCtrlButtons, Keycode); 12] = [
    (SCE_CTRL_UP, Keycode::Up),
    (SCE_CTRL_DOWN, Keycode::Down),
    (SCE_CTRL_LEFT, Keycode::Left),
    (SCE_CTRL_RIGHT, Keycode::Right),
    (SCE_CTRL_START, Keycode::Start),
    (SCE_CTRL_SELECT, Keycode::Select),
    (SCE_CTRL_CIRCLE, Keycode::A),
    (SCE_CTRL_CROSS, Keycode::B),
    (SCE_CTRL_TRIANGLE, Keycode::X),
    (SCE_CTRL_SQUARE, Keycode::Y),
    (SCE_CTRL_LTRIGGER, Keycode::TriggerL),
    (SCE_CTRL_RTRIGGER, Keycode::TriggerR),
];

pub struct Presenter {
    key_map: Arc<AtomicU16>,
    pgf: *mut Vita2dPgf,
    top_texture: *mut Vita2dTexture,
    bottom_texture: *mut Vita2dTexture,
    top_texture_data_ptr: *mut u32,
    bottom_texture_data_ptr: *mut u32,
}

impl Presenter {
    pub fn new(key_map: Arc<AtomicU16>) -> Self {
        unsafe {
            vita2d_init();
            vita2d_set_clear_color(rgba8(0, 0, 0, 255));
            let pgf = vita2d_load_default_pgf();
            let top_texture = vita2d_create_empty_texture(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
            let bottom_texture =
                vita2d_create_empty_texture(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);

            Presenter {
                key_map,
                pgf,
                top_texture,
                bottom_texture,
                top_texture_data_ptr: vita2d_texture_get_datap(top_texture) as *mut u32,
                bottom_texture_data_ptr: vita2d_texture_get_datap(bottom_texture) as *mut u32,
            }
        }
    }

    pub fn event_poll(&mut self) -> bool {
        unsafe {
            let pressed = MaybeUninit::<SceCtrlData>::uninit();
            let mut pressed = pressed.assume_init();
            sceCtrlPeekBufferPositive(0, ptr::addr_of_mut!(pressed), 1);

            for (host_key, guest_key) in KEY_CODE_MAPPING {
                if pressed.buttons & host_key != 0 {
                    self.key_map
                        .fetch_and(!(1 << guest_key as u8), Ordering::Relaxed);
                } else {
                    self.key_map
                        .fetch_or(1 << guest_key as u8, Ordering::Relaxed);
                }
            }
        }
        true
    }

    pub fn present(
        &mut self,
        top: &[u32; DISPLAY_PIXEL_COUNT],
        bottom: &[u32; DISPLAY_PIXEL_COUNT],
        fps: u16,
    ) {
        let fps_str = CString::new(format!("Internal fps {}", fps)).unwrap();

        unsafe {
            self.top_texture_data_ptr
                .copy_from(top.as_ptr(), DISPLAY_PIXEL_COUNT);
            self.bottom_texture_data_ptr
                .copy_from(bottom.as_ptr(), DISPLAY_PIXEL_COUNT);

            vita2d_start_drawing();
            vita2d_clear_screen();

            const ADJUSTED_DISPLAY_HEIGHT: u32 =
                PRESENTER_SCREEN_WIDTH / 2 * DISPLAY_HEIGHT as u32 / DISPLAY_WIDTH as u32;

            vita2d_draw_texture_part_scale(
                self.top_texture,
                0f32,
                ((PRESENTER_SCREEN_HEIGHT - ADJUSTED_DISPLAY_HEIGHT) / 2) as _,
                0f32,
                0f32,
                DISPLAY_WIDTH as f32,
                DISPLAY_HEIGHT as f32,
                (PRESENTER_SCREEN_WIDTH as f32 / 2f32) / DISPLAY_WIDTH as f32,
                ADJUSTED_DISPLAY_HEIGHT as f32 / DISPLAY_HEIGHT as f32,
            );

            vita2d_draw_texture_part_scale(
                self.bottom_texture,
                PRESENTER_SCREEN_WIDTH as f32 / 2f32,
                ((PRESENTER_SCREEN_HEIGHT - ADJUSTED_DISPLAY_HEIGHT) / 2) as _,
                0f32,
                0f32,
                DISPLAY_WIDTH as f32,
                DISPLAY_HEIGHT as f32,
                (PRESENTER_SCREEN_WIDTH as f32 / 2f32) / DISPLAY_WIDTH as f32,
                ADJUSTED_DISPLAY_HEIGHT as f32 / DISPLAY_HEIGHT as f32,
            );

            vita2d_pgf_draw_text(
                self.pgf,
                (PRESENTER_SCREEN_WIDTH - 170) as _,
                40,
                rgba8(0, 255, 0, 255),
                1f32,
                fps_str.as_c_str().as_ptr(),
            );

            vita2d_end_drawing();
            vita2d_swap_buffers();
        }
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        unsafe {
            vita2d_fini();
            vita2d_free_texture(self.top_texture);
            vita2d_free_texture(self.bottom_texture);
            vita2d_free_pgf(self.pgf);
        }
    }
}

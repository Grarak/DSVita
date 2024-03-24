#![allow(incomplete_features)]
#![feature(adt_const_params)]
#![feature(arm_target_feature)]
#![feature(const_trait_impl)]
#![feature(core_intrinsics)]
#![feature(generic_const_exprs)]
#![feature(isqrt)]
#![feature(naked_functions)]
#![feature(rustc_attrs)]
#![feature(seek_stream_len)]
#![feature(stmt_expr_attributes)]
#![feature(thread_id_value)]

extern crate core;

use crate::cartridge_reader::CartridgeReader;
use crate::hle::gpu::gpu::{Gpu, Swapchain, DISPLAY_HEIGHT, DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH};
use crate::hle::hle::{get_cm, get_cp15_mut, get_cpu_regs, get_mmu, get_regs_mut, Hle};
use crate::hle::{input, spi, CpuType};
use crate::jit::jit_asm::JitAsm;
use crate::utils::{set_thread_prio_affinity, BuildNoHasher, ThreadAffinity, ThreadPriority};
use sdl2::event::Event;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use std::cmp::min;
use std::collections::HashMap;
use std::intrinsics::{likely, unlikely};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::{mem, ptr, thread};
use CpuType::{ARM7, ARM9};

mod cartridge_reader;
mod hle;
mod jit;
mod logging;
mod mmap;
mod utils;

#[cfg(target_os = "vita")]
#[link(
    name = "SceAudioIn_stub",
    kind = "static",
    modifiers = "+whole-archive"
)]
#[link(name = "SceAudio_stub", kind = "static", modifiers = "+whole-archive")]
#[link(
    name = "SceCommonDialog_stub",
    kind = "static",
    modifiers = "+whole-archive"
)]
#[link(name = "SceCtrl_stub", kind = "static", modifiers = "+whole-archive")]
#[link(
    name = "SceDisplay_stub",
    kind = "static",
    modifiers = "+whole-archive"
)]
#[link(name = "SceGxm_stub", kind = "static", modifiers = "+whole-archive")]
#[link(name = "SceHid_stub", kind = "static", modifiers = "+whole-archive")]
#[link(name = "SceMotion_stub", kind = "static", modifiers = "+whole-archive")]
#[link(name = "SceTouch_stub", kind = "static", modifiers = "+whole-archive")]
extern "C" {}

pub const DEBUG_LOG: bool = cfg!(debug_assertions);

const SCREEN_WIDTH: u32 = 960;
const SCREEN_HEIGHT: u32 = 544;

#[cfg(target_os = "linux")]
fn get_file_path() -> String {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 {
        args[1].clone()
    } else {
        eprintln!("Usage {} <path_to_nds>", args[0]);
        std::process::exit(1);
    }
}

#[cfg(target_os = "vita")]
fn get_file_path() -> String {
    "ux0:ace_attorney.nds".to_owned()
}

fn run_cpu(cartridge_reader: CartridgeReader, swapchain: Arc<Swapchain>, key_map: Arc<AtomicU16>) {
    let arm9_ram_addr = cartridge_reader.header.arm9_values.ram_address;
    let arm9_entry_addr = cartridge_reader.header.arm9_values.entry_address;
    let arm7_ram_addr = cartridge_reader.header.arm7_values.ram_address;
    let arm7_entry_addr = cartridge_reader.header.arm7_values.entry_address;

    let mut hle = Hle::new(cartridge_reader, swapchain, key_map);

    {
        let cartridge_header: &[u8; cartridge_reader::HEADER_IN_RAM_SIZE] =
            unsafe { mem::transmute(&hle.common.cartridge.reader.header) };
        hle.mem.main.write_slice(0x7FFE00, cartridge_header);

        hle.mem.main.write(0x27FF850, 0x5835u16); // ARM7 BIOS CRC
        hle.mem.main.write(0x27FF880, 0x0007u16); // Message from ARM9 to ARM7
        hle.mem.main.write(0x27FF884, 0x0006u16); // ARM7 boot task
        hle.mem.main.write(0x27FFC10, 0x5835u16); // Copy of ARM7 BIOS CRC
        hle.mem.main.write(0x27FFC40, 0x0001u16); // Boot indicator

        hle.mem.main.write(0x27FF800, 0x00001FC2u32); // Chip ID 1
        hle.mem.main.write(0x27FF804, 0x00001FC2u32); // Chip ID 2
        hle.mem.main.write(0x27FFC00, 0x00001FC2u32); // Copy of chip ID 1
        hle.mem.main.write(0x27FFC04, 0x00001FC2u32); // Copy of chip ID 2

        // User settings
        hle.mem.main.write_slice(
            0x27FFC80,
            &spi::SPI_FIRMWARE[spi::USER_SETTINGS_1_ADDR..spi::USER_SETTINGS_1_ADDR + 0x70],
        );
    }

    {
        let hle_ptr = ptr::addr_of!(hle);
        let cp15 = get_cp15_mut!(hle, ARM9);
        let hle_tmp = unsafe { hle_ptr.as_ref().unwrap_unchecked() };
        cp15.write(0x010000, 0x0005707D, hle_tmp); // control
        cp15.write(0x090100, 0x0300000A, hle_tmp); // dtcm addr/size
        cp15.write(0x090101, 0x00000020, hle_tmp); // itcm size
    }

    {
        // I/O Ports
        hle.mem_write::<{ ARM9 }, _>(0x4000247, 0x03u8);
        hle.mem_write::<{ ARM9 }, _>(0x4000300, 0x01u8);
        hle.mem_write::<{ ARM9 }, _>(0x4000304, 0x0001u16);
    }

    {
        let regs = get_regs_mut!(hle, ARM9);
        regs.user.gp_regs[4] = arm9_entry_addr; // R12
        regs.user.sp = 0x3002F7C;
        regs.irq.sp = 0x3003F80;
        regs.svc.sp = 0x3003FC0;
        regs.user.lr = arm9_entry_addr;
        regs.pc = arm9_entry_addr;
        regs.set_cpsr::<false>(0x000000DF, get_cm!(hle));
    }

    {
        // I/O Ports
        hle.mem_write::<{ ARM7 }, _>(0x4000300, 0x01u8); // POWCNT1
        hle.mem_write::<{ ARM7 }, _>(0x4000504, 0x0200u16); // SOUNDBIAS
    }

    {
        let regs = get_regs_mut!(hle, ARM7);
        regs.user.gp_regs[4] = arm7_entry_addr; // R12
        regs.user.sp = 0x380FD80;
        regs.irq.sp = 0x380FF80;
        regs.user.sp = 0x380FFC0;
        regs.user.lr = arm7_entry_addr;
        regs.pc = arm7_entry_addr;
        regs.set_cpsr::<false>(0x000000DF, get_cm!(hle));
    }

    {
        let arm9_code = hle.common.cartridge.reader.read_arm9_code();
        let arm7_code = hle.common.cartridge.reader.read_arm7_code();

        for (i, value) in arm9_code.iter().enumerate() {
            hle.mem_write::<{ ARM9 }, _>(arm9_ram_addr + i as u32, *value);
        }

        for (i, value) in arm7_code.iter().enumerate() {
            hle.mem_write::<{ ARM7 }, _>(arm7_ram_addr + i as u32, *value);
        }
    }

    Gpu::initialize_schedule(get_cm!(hle));

    let hle_ptr = ptr::addr_of_mut!(hle) as u32;
    let gpu2d_thread = thread::Builder::new()
        .name("gpu2d".to_owned())
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core1);
            let hle = unsafe { (hle_ptr as *mut Hle).as_mut().unwrap() };
            loop {
                hle.common.gpu.draw_scanline_thread(&hle.mem);
            }
        })
        .unwrap();

    execute_jit(&mut hle);
    gpu2d_thread.join().unwrap();
}

#[inline(never)]
fn execute_jit(hle: &mut Hle) {
    let hle_ptr = hle as *mut Hle;
    let mut jit_asm_arm9 = JitAsm::<{ ARM9 }>::new(unsafe { hle_ptr.as_mut().unwrap() });
    let mut jit_asm_arm7 = JitAsm::<{ ARM7 }>::new(unsafe { hle_ptr.as_mut().unwrap() });
    hle.mem.jit.open();

    get_mmu!(jit_asm_arm9.hle, ARM9).update_all(hle);
    get_mmu!(jit_asm_arm7.hle, ARM7).update_all(hle);

    loop {
        let mut arm9_cycles = if likely(!get_cpu_regs!(hle, ARM9).is_halted()) {
            (jit_asm_arm9.execute() + 1) >> 1
        } else {
            0
        };

        let arm7_cycles = if likely(!get_cpu_regs!(hle, ARM7).is_halted()) {
            jit_asm_arm7.execute()
        } else {
            0
        };

        while likely(!get_cpu_regs!(hle, ARM9).is_halted()) && (arm7_cycles > arm9_cycles) {
            arm9_cycles += (jit_asm_arm9.execute() + 1) >> 1;
        }

        let cycles = min(arm9_cycles.wrapping_sub(1), arm7_cycles.wrapping_sub(1)).wrapping_add(1);
        if unlikely(cycles == 0) {
            hle.common.cycle_manager.jump_to_next_event();
        } else {
            hle.common.cycle_manager.add_cycle(cycles);
        }
        get_cm!(hle).check_events(jit_asm_arm9.hle);
    }
}

// Must be pub for vita
pub fn main() {
    set_thread_prio_affinity(ThreadPriority::Low, ThreadAffinity::Core0);
    
    if DEBUG_LOG {
        std::env::set_var("RUST_BACKTRACE", "full");
    }

    let cartridge_reader = CartridgeReader::from_file(&get_file_path()).unwrap();

    let swapchain = Arc::new(Swapchain::new());
    let swapchain_clone = swapchain.clone();

    let key_map = Arc::new(AtomicU16::new(0xFFFF));
    let key_map_clone = key_map.clone();

    let cpu_thread = thread::Builder::new()
        .name("cpu".to_owned())
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core2);
            run_cpu(cartridge_reader, swapchain_clone, key_map_clone)
        })
        .unwrap();

    sdl2::hint::set("SDL_NO_SIGNAL_HANDLERS", "1");
    let sdl = sdl2::init().unwrap();
    let sdl_video = sdl.video().unwrap();

    let _controller = if let Ok(controller_subsystem) = sdl.game_controller() {
        if let Ok(available) = controller_subsystem.num_joysticks() {
            (0..available).find_map(|id| {
                if !controller_subsystem.is_game_controller(id) {
                    None
                } else {
                    controller_subsystem.open(id).ok()
                }
            })
        } else {
            None
        }
    } else {
        None
    };

    let sdl_window = sdl_video
        .window("DSPSV", SCREEN_WIDTH, SCREEN_HEIGHT)
        .build()
        .unwrap();
    #[cfg(target_os = "linux")]
    let mut sdl_canvas = sdl_window
        .into_canvas()
        .software()
        .target_texture()
        .build()
        .unwrap();
    #[cfg(target_os = "vita")]
    let mut sdl_canvas = sdl_window.into_canvas().target_texture().build().unwrap();
    let sdl_texture_creator = sdl_canvas.texture_creator();
    let mut sdl_texture_top = sdl_texture_creator
        .create_texture_streaming(
            PixelFormatEnum::ABGR8888,
            DISPLAY_WIDTH as u32,
            DISPLAY_HEIGHT as u32,
        )
        .unwrap();
    let mut sdl_texture_bottom = sdl_texture_creator
        .create_texture_streaming(
            PixelFormatEnum::ABGR8888,
            DISPLAY_WIDTH as u32,
            DISPLAY_HEIGHT as u32,
        )
        .unwrap();
    sdl_canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
    sdl_canvas.clear();
    sdl_canvas.present();

    let mut key_code_mapping = HashMap::<_, _, BuildNoHasher>::default();
    #[cfg(target_os = "linux")]
    {
        use sdl2::keyboard::Keycode;
        key_code_mapping.insert(Keycode::W, input::Keycode::Up);
        key_code_mapping.insert(Keycode::S, input::Keycode::Down);
        key_code_mapping.insert(Keycode::A, input::Keycode::Left);
        key_code_mapping.insert(Keycode::D, input::Keycode::Right);
        key_code_mapping.insert(Keycode::B, input::Keycode::Start);
        key_code_mapping.insert(Keycode::K, input::Keycode::A);
        key_code_mapping.insert(Keycode::J, input::Keycode::B);
    }
    #[cfg(target_os = "vita")]
    {
        use sdl2::controller::Button;
        key_code_mapping.insert(Button::DPadUp, input::Keycode::Up);
        key_code_mapping.insert(Button::DPadDown, input::Keycode::Down);
        key_code_mapping.insert(Button::DPadLeft, input::Keycode::Left);
        key_code_mapping.insert(Button::DPadRight, input::Keycode::Right);
        key_code_mapping.insert(Button::Start, input::Keycode::Start);
        key_code_mapping.insert(Button::A, input::Keycode::A);
        key_code_mapping.insert(Button::B, input::Keycode::B);
    }

    let mut sdl_event_pump = sdl.event_pump().unwrap();

    'render: loop {
        for event in sdl_event_pump.poll_iter() {
            match event {
                #[cfg(target_os = "linux")]
                Event::KeyDown {
                    keycode: Some(code),
                    ..
                } => {
                    if let Some(code) = key_code_mapping.get(&code) {
                        key_map.fetch_and(!(1 << *code as u8), Ordering::Relaxed);
                    }
                }
                #[cfg(target_os = "linux")]
                Event::KeyUp {
                    keycode: Some(code),
                    ..
                } => {
                    if let Some(code) = key_code_mapping.get(&code) {
                        key_map.fetch_or(1 << *code as u8, Ordering::Relaxed);
                    }
                }
                #[cfg(target_os = "vita")]
                Event::ControllerButtonDown { button, .. } => {
                    if let Some(code) = key_code_mapping.get(&button) {
                        key_map.fetch_and(!(1 << *code as u8), Ordering::Relaxed);
                    }
                }
                #[cfg(target_os = "vita")]
                Event::ControllerButtonUp { button, .. } => {
                    if let Some(code) = key_code_mapping.get(&button) {
                        key_map.fetch_or(1 << *code as u8, Ordering::Relaxed);
                    }
                }
                Event::Quit { .. } => break 'render,
                _ => {}
            }
        }

        let fb = swapchain.consume();
        let top_aligned: &[u8] = unsafe { mem::transmute(&fb[..DISPLAY_PIXEL_COUNT]) };
        let bottom_aligned: &[u8] = unsafe { mem::transmute(&fb[DISPLAY_PIXEL_COUNT..]) };
        sdl_texture_top
            .update(None, top_aligned, DISPLAY_WIDTH * 4)
            .unwrap();
        sdl_texture_bottom
            .update(None, bottom_aligned, DISPLAY_WIDTH * 4)
            .unwrap();

        sdl_canvas.clear();
        const ADJUSTED_DISPLAY_HEIGHT: u32 =
            SCREEN_WIDTH / 2 * DISPLAY_HEIGHT as u32 / DISPLAY_WIDTH as u32;
        sdl_canvas
            .copy(
                &sdl_texture_top,
                None,
                Some(Rect::new(
                    0,
                    ((SCREEN_HEIGHT - ADJUSTED_DISPLAY_HEIGHT) / 2) as _,
                    SCREEN_WIDTH / 2,
                    ADJUSTED_DISPLAY_HEIGHT,
                )),
            )
            .unwrap();
        sdl_canvas
            .copy(
                &sdl_texture_bottom,
                None,
                Some(Rect::new(
                    SCREEN_WIDTH as i32 / 2,
                    ((SCREEN_HEIGHT - ADJUSTED_DISPLAY_HEIGHT) / 2) as _,
                    SCREEN_WIDTH / 2,
                    ADJUSTED_DISPLAY_HEIGHT,
                )),
            )
            .unwrap();
        sdl_canvas.present();
    }

    cpu_thread.join().unwrap();
}

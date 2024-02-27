#![allow(incomplete_features)]
#![feature(adt_const_params)]
#![feature(arm_target_feature)]
#![feature(const_trait_impl)]
#![feature(generic_const_exprs)]
#![feature(isqrt)]
#![feature(naked_functions)]
#![feature(seek_stream_len)]
#![feature(stmt_expr_attributes)]
#![feature(thread_id_value)]

use crate::cartridge::Cartridge;
use crate::hle::cp15_context::Cp15Context;
use crate::hle::cpu_regs::{CpuRegs, CpuRegsContainer};
use crate::hle::cycle_manager::CycleManager;
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_3d_context::Gpu3DContext;
use crate::hle::gpu::gpu_context::{
    GpuContext, Swapchain, DISPLAY_HEIGHT, DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH,
};
use crate::hle::input_context::InputContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::cartridge_context::CartridgeContext;
use crate::hle::memory::dma::{Dma, DmaContainer};
use crate::hle::memory::main_memory::MainMemory;
use crate::hle::memory::oam_context::OamContext;
use crate::hle::memory::palettes_context::PalettesContext;
use crate::hle::memory::tcm_context::TcmContext;
use crate::hle::memory::vram_context::VramContext;
use crate::hle::memory::wram_context::WramContext;
use crate::hle::rtc_context::RtcContext;
use crate::hle::spi_context::SpiContext;
use crate::hle::spu_context::SpuContext;
use crate::hle::thread_context::ThreadContext;
use crate::hle::CpuType;
use crate::hle::{input_context, spi_context};
use crate::jit::jit_memory::JitMemory;
use crate::utils::BuildNoHasher;
use sdl2::event::Event;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use std::cell::RefCell;
use std::cmp::min;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::{mem, ptr, thread};

mod cartridge;
mod hle;
mod jit;
mod logging;
mod mmap;
mod simple_tree_map;
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
    "ux0:hello_world.nds".to_owned()
}

fn initialize_arm9_thread(entry_addr: u32, thread: &ThreadContext<{ CpuType::ARM9 }>) {
    {
        let mut cp15 = thread.cp15_context.borrow_mut();
        cp15.write(0x010000, 0x0005707D); // control
        cp15.write(0x090100, 0x0300000A); // dtcm addr/size
        cp15.write(0x090101, 0x00000020); // itcm size
    }

    {
        // I/O Ports
        thread.mem_handler.write(0x4000247, 0x03u8);
        thread.mem_handler.write(0x4000300, 0x01u8);
        thread.mem_handler.write(0x4000304, 0x0001u16);
    }

    {
        let mut regs = thread.regs.borrow_mut();
        regs.user.gp_regs[4] = entry_addr; // R12
        regs.user.sp = 0x3002F7C;
        regs.irq.sp = 0x3003F80;
        regs.svc.sp = 0x3003FC0;
        regs.user.lr = entry_addr;
        regs.pc = entry_addr;
        regs.set_cpsr::<false>(0x000000DF);
    }
}

fn initialize_arm7_thread(entry_addr: u32, thread: &ThreadContext<{ CpuType::ARM7 }>) {
    {
        // I/O Ports
        thread.mem_handler.write(0x4000300, 0x01u8); // POWCNT1
        thread.mem_handler.write(0x4000504, 0x0200u16); // SOUNDBIAS
    }

    {
        let mut regs = thread.regs.borrow_mut();
        regs.user.gp_regs[4] = entry_addr; // R12
        regs.user.sp = 0x380FD80;
        regs.irq.sp = 0x380FF80;
        regs.user.sp = 0x380FFC0;
        regs.user.lr = entry_addr;
        regs.pc = entry_addr;
        regs.set_cpsr::<false>(0x000000DF);
    }
}

// Must be pub for vita
pub fn main() {
    #[cfg(debug_assertions)]
    std::env::set_var("RUST_BACKTRACE", "full");

    let cartridge = Cartridge::from_file(&get_file_path()).unwrap();

    let arm9_ram_addr = cartridge.header.arm9_values.ram_address;
    let arm9_entry_adrr = cartridge.header.arm9_values.entry_address;
    let arm7_ram_addr = cartridge.header.arm7_values.ram_address;
    let arm7_entry_addr = cartridge.header.arm7_values.entry_address;

    let mut main_memory = MainMemory::new();
    {
        let header: &[u8; cartridge::HEADER_IN_RAM_SIZE] =
            unsafe { mem::transmute(&cartridge.header) };
        main_memory.write_slice(0x7FFE00, header);

        main_memory.write(0x27FF850, 0x5835u16); // ARM7 BIOS CRC
        main_memory.write(0x27FF880, 0x0007u16); // Message from ARM9 to ARM7
        main_memory.write(0x27FF884, 0x0006u16); // ARM7 boot task
        main_memory.write(0x27FFC10, 0x5835u16); // Copy of ARM7 BIOS CRC
        main_memory.write(0x27FFC40, 0x0001u16); // Boot indicator

        main_memory.write(0x27FF800, 0x00001FC2u32); // Chip ID 1
        main_memory.write(0x27FF804, 0x00001FC2u32); // Chip ID 2
        main_memory.write(0x27FFC00, 0x00001FC2u32); // Copy of chip ID 1
        main_memory.write(0x27FFC04, 0x00001FC2u32); // Copy of chip ID 2

        // User settings
        main_memory.write_slice(
            0x27FFC80,
            &spi_context::SPI_FIRMWARE
                [spi_context::USER_SETTINGS_1_ADDR..spi_context::USER_SETTINGS_1_ADDR + 0x70],
        );
    }

    let cycle_manager = Rc::new(CycleManager::new());
    let jit_memory = Rc::new(RefCell::new(JitMemory::new()));
    let wram_context = Rc::new(RefCell::new(WramContext::new()));
    let spi_context = Rc::new(RefCell::new(SpiContext::new()));
    let vram_context = Rc::new(RefCell::new(VramContext::new()));
    let input_context = Arc::new(RwLock::new(InputContext::new()));
    let rtc_context = Rc::new(RefCell::new(RtcContext::new()));
    let spu_context = Rc::new(RefCell::new(SpuContext::new()));
    let palettes_context = Rc::new(RefCell::new(PalettesContext::new()));
    let tcm_context = Rc::new(RefCell::new(TcmContext::new()));
    let oam_context = Rc::new(RefCell::new(OamContext::new()));
    let cpu_regs_arm9 = Rc::new(CpuRegs::new(cycle_manager.clone()));
    let cpu_regs_arm7 = Rc::new(CpuRegs::new(cycle_manager.clone()));
    let ipc_handler = Rc::new(RefCell::new(IpcHandler::new(CpuRegsContainer::new(
        cpu_regs_arm9.clone(),
        cpu_regs_arm7.clone(),
    ))));
    let cp15_context = Rc::new(RefCell::new(Cp15Context::new()));

    let gpu_2d_context_a = Rc::new(RefCell::new(Gpu2DContext::new(
        vram_context.clone(),
        palettes_context.clone(),
    )));
    let gpu_2d_context_b = Rc::new(RefCell::new(Gpu2DContext::new(
        vram_context.clone(),
        palettes_context.clone(),
    )));
    let gpu3d_context = Rc::new(RefCell::new(Gpu3DContext::new()));
    let dma_arm9 = Rc::new(RefCell::new(Dma::new(cycle_manager.clone())));
    let dma_arm7 = Rc::new(RefCell::new(Dma::new(cycle_manager.clone())));
    let swapchain = Arc::new(Swapchain::new());
    let gpu_context = Arc::new(GpuContext::new(
        cycle_manager.clone(),
        gpu_2d_context_a.clone(),
        gpu_2d_context_b.clone(),
        dma_arm9.clone(),
        dma_arm7.clone(),
        cpu_regs_arm9.clone(),
        cpu_regs_arm7.clone(),
        swapchain.clone(),
    ));
    let cartridge_context = Rc::new(RefCell::new(CartridgeContext::new(
        cycle_manager.clone(),
        CpuRegsContainer::new(cpu_regs_arm9.clone(), cpu_regs_arm7.clone()),
        DmaContainer::new(dma_arm9.clone(), dma_arm7.clone()),
        cartridge,
    )));

    let mut arm9_thread = ThreadContext::<{ CpuType::ARM9 }>::new(
        cycle_manager.clone(),
        jit_memory.clone(),
        ptr::addr_of_mut!(main_memory),
        wram_context.clone(),
        spi_context.clone(),
        ipc_handler.clone(),
        vram_context.clone(),
        input_context.clone(),
        gpu_context.clone(),
        gpu_2d_context_a.clone(),
        gpu_2d_context_b.clone(),
        gpu3d_context.clone(),
        dma_arm9,
        rtc_context.clone(),
        spu_context.clone(),
        palettes_context.clone(),
        cp15_context.clone(),
        tcm_context.clone(),
        oam_context.clone(),
        cpu_regs_arm9,
        cartridge_context.clone(),
    );
    initialize_arm9_thread(arm9_entry_adrr, &arm9_thread);

    let mut arm7_thread = ThreadContext::<{ CpuType::ARM7 }>::new(
        cycle_manager,
        jit_memory,
        ptr::addr_of_mut!(main_memory),
        wram_context,
        spi_context,
        ipc_handler,
        vram_context,
        input_context.clone(),
        gpu_context.clone(),
        gpu_2d_context_a.clone(),
        gpu_2d_context_b.clone(),
        gpu3d_context,
        dma_arm7,
        rtc_context,
        spu_context,
        palettes_context,
        cp15_context,
        tcm_context,
        oam_context,
        cpu_regs_arm7,
        cartridge_context.clone(),
    );
    initialize_arm7_thread(arm7_entry_addr, &arm7_thread);

    {
        let arm9_code = cartridge_context.borrow().cartridge.read_arm9_code();
        for (i, value) in arm9_code.iter().enumerate() {
            arm9_thread
                .mem_handler
                .write(arm9_ram_addr + i as u32, *value);
        }
    }

    {
        let arm7_code = cartridge_context.borrow().cartridge.read_arm7_code();
        for (i, value) in arm7_code.iter().enumerate() {
            arm7_thread
                .mem_handler
                .write(arm7_ram_addr + i as u32, *value);
        }
    }

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

    let cpu_thread = thread::Builder::new()
        .name("cpu".to_owned())
        .spawn(move || {
            arm9_thread.jit.jit_memory.borrow_mut().open();
            let cycle_manager = arm9_thread.cycle_manager.clone();
            loop {
                let arm7_cycles = if !arm7_thread.is_halted() {
                    arm7_thread.run()
                } else {
                    0
                };

                let mut arm9_cycles = 0;
                while !arm9_thread.is_halted() && (arm7_cycles > arm9_cycles || arm9_cycles == 0) {
                    arm9_cycles += arm9_thread.run();
                }

                let cycles =
                    min(arm9_cycles.wrapping_sub(1), arm7_cycles.wrapping_sub(1)).wrapping_add(1);
                if cycles == 0 {
                    cycle_manager.jump_to_next_event();
                } else {
                    cycle_manager.add_cycle(cycles);
                }
                cycle_manager.check_events();
            }
        })
        .unwrap();

    let mut key_code_mapping = HashMap::<_, _, BuildNoHasher>::default();
    #[cfg(target_os = "linux")]
    {
        use sdl2::keyboard::Keycode;
        key_code_mapping.insert(Keycode::W, input_context::Keycode::Up);
        key_code_mapping.insert(Keycode::S, input_context::Keycode::Down);
        key_code_mapping.insert(Keycode::A, input_context::Keycode::Left);
        key_code_mapping.insert(Keycode::D, input_context::Keycode::Right);
        key_code_mapping.insert(Keycode::B, input_context::Keycode::Start);
        key_code_mapping.insert(Keycode::K, input_context::Keycode::A);
        key_code_mapping.insert(Keycode::J, input_context::Keycode::B);
    }
    #[cfg(target_os = "vita")]
    {
        use sdl2::controller::Button;
        key_code_mapping.insert(Button::DPadUp, input_context::Keycode::Up);
        key_code_mapping.insert(Button::DPadDown, input_context::Keycode::Down);
        key_code_mapping.insert(Button::DPadLeft, input_context::Keycode::Left);
        key_code_mapping.insert(Button::DPadRight, input_context::Keycode::Right);
        key_code_mapping.insert(Button::Start, input_context::Keycode::Start);
        key_code_mapping.insert(Button::A, input_context::Keycode::A);
        key_code_mapping.insert(Button::B, input_context::Keycode::B);
    }

    let mut sdl_event_pump = sdl.event_pump().unwrap();
    let mut key_map = 0xFFFF;

    'render: loop {
        for event in sdl_event_pump.poll_iter() {
            match event {
                #[cfg(target_os = "linux")]
                Event::KeyDown {
                    keycode: Some(code),
                    ..
                } => {
                    if let Some(code) = key_code_mapping.get(&code) {
                        key_map &= !(1 << *code as u8);
                    }
                }
                #[cfg(target_os = "linux")]
                Event::KeyUp {
                    keycode: Some(code),
                    ..
                } => {
                    if let Some(code) = key_code_mapping.get(&code) {
                        key_map |= 1 << *code as u8;
                    }
                }
                #[cfg(target_os = "vita")]
                Event::ControllerButtonDown { button, .. } => {
                    if let Some(code) = key_code_mapping.get(&button) {
                        key_map &= !(1 << *code as u8);
                    }
                }
                #[cfg(target_os = "vita")]
                Event::ControllerButtonUp { button, .. } => {
                    if let Some(code) = key_code_mapping.get(&button) {
                        key_map |= 1 << *code as u8;
                    }
                }
                Event::Quit { .. } => break 'render,
                _ => {}
            }
        }

        input_context.write().unwrap().update_key_map(key_map);

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

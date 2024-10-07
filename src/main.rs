#![allow(incomplete_features)]
#![allow(internal_features)]
#![feature(adt_const_params)]
#![feature(allocator_api)]
#![feature(arm_target_feature)]
#![feature(const_trait_impl)]
#![feature(core_intrinsics)]
#![feature(generic_const_exprs)]
#![feature(isqrt)]
#![feature(naked_functions)]
#![feature(new_zeroed_alloc)]
#![feature(ptr_as_ref_unchecked)]
#![feature(seek_stream_len)]
#![feature(stmt_expr_attributes)]

use crate::cartridge_io::CartridgeIo;
use crate::core::emu::{get_cm_mut, get_common_mut, get_cp15_mut, get_cpu_regs, get_jit_mut, get_mem_mut, get_mmu, get_regs_mut, get_spu_mut, Emu};
use crate::core::graphics::gpu::Gpu;
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::spu::{SoundSampler, Spu};
use crate::core::{spi, CpuType};
use crate::jit::jit_asm::JitAsm;
use crate::logging::debug_println;
use crate::presenter::{PresentEvent, Presenter, PRESENTER_AUDIO_BUF_SIZE};
use crate::settings::Settings;
use crate::utils::{set_thread_prio_affinity, HeapMemU32, ThreadAffinity, ThreadPriority};
use std::cell::UnsafeCell;
use std::cmp::min;
use std::intrinsics::{likely, unlikely};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{mem, ptr, thread};
use CpuType::{ARM7, ARM9};

mod cartridge_io;
mod cartridge_metadata;
mod core;
mod jit;
mod logging;
mod math;
mod mmap;
mod presenter;
mod settings;
mod utils;

pub const DEBUG_LOG: bool = cfg!(debug_assertions);
pub const DEBUG_LOG_BRANCH_OUT: bool = DEBUG_LOG;

fn run_cpu(
    cartridge_io: CartridgeIo,
    fps: Arc<AtomicU16>,
    key_map: Arc<AtomicU32>,
    touch_points: Arc<AtomicU16>,
    sound_sampler: Arc<SoundSampler>,
    settings: Settings,
    gpu_renderer: NonNull<GpuRenderer>,
    last_save_time: Arc<Mutex<Option<(Instant, bool)>>>,
) {
    let arm9_ram_addr = cartridge_io.header.arm9_values.ram_address;
    let arm9_entry_addr = cartridge_io.header.arm9_values.entry_address;
    let arm7_ram_addr = cartridge_io.header.arm7_values.ram_address;
    let arm7_entry_addr = cartridge_io.header.arm7_values.entry_address;

    let mut emu_unsafe = UnsafeCell::new(Emu::new(cartridge_io, fps, key_map, touch_points, sound_sampler, settings));
    let emu_ptr = emu_unsafe.get() as u32;
    let emu = emu_unsafe.get_mut();
    let common = get_common_mut!(emu);
    let mem = get_mem_mut!(emu);

    {
        let cartridge_header: &[u8; cartridge_io::HEADER_IN_RAM_SIZE] = unsafe { mem::transmute(&common.cartridge.io.header) };
        mem.main.write_slice(0x7FFE00, cartridge_header);

        mem.main.write(0x27FF850, 0x5835u16); // ARM7 BIOS CRC
        mem.main.write(0x27FF880, 0x0007u16); // Message from ARM9 to ARM7
        mem.main.write(0x27FF884, 0x0006u16); // ARM7 boot task
        mem.main.write(0x27FFC10, 0x5835u16); // Copy of ARM7 BIOS CRC
        mem.main.write(0x27FFC40, 0x0001u16); // Boot indicator

        mem.main.write(0x27FF800, 0x00001FC2u32); // Chip ID 1
        mem.main.write(0x27FF804, 0x00001FC2u32); // Chip ID 2
        mem.main.write(0x27FFC00, 0x00001FC2u32); // Copy of chip ID 1
        mem.main.write(0x27FFC04, 0x00001FC2u32); // Copy of chip ID 2

        // User settings
        mem.main.write_slice(0x27FFC80, &spi::SPI_FIRMWARE[spi::USER_SETTINGS_1_ADDR..spi::USER_SETTINGS_1_ADDR + 0x70]);
    }

    {
        let cp15 = get_cp15_mut!(emu, ARM9);
        cp15.write(0x010000, 0x0005707D, emu); // control
        cp15.write(0x090100, 0x0300000A, emu); // dtcm addr/size
        cp15.write(0x090101, 0x00000020, emu); // itcm size
    }

    {
        // I/O Ports
        emu.mem_write::<{ ARM9 }, _>(0x4000247, 0x03u8);
        emu.mem_write::<{ ARM9 }, _>(0x4000300, 0x01u8);
        emu.mem_write::<{ ARM9 }, _>(0x4000304, 0x0001u16);
    }

    {
        let regs = get_regs_mut!(emu, ARM9);
        regs.user.gp_regs[4] = arm9_entry_addr; // R12
        regs.user.sp = 0x3002F7C;
        regs.irq.sp = 0x3003F80;
        regs.svc.sp = 0x3003FC0;
        regs.user.lr = arm9_entry_addr;
        regs.pc = arm9_entry_addr;
        regs.set_cpsr::<false>(0x000000DF, emu);
    }

    {
        // I/O Ports
        emu.mem_write::<{ ARM7 }, _>(0x4000300, 0x01u8); // POWCNT1
        emu.mem_write::<{ ARM7 }, _>(0x4000504, 0x0200u16); // SOUNDBIAS
    }

    {
        let regs = get_regs_mut!(emu, ARM7);
        regs.user.gp_regs[4] = arm7_entry_addr; // R12
        regs.user.sp = 0x380FD80;
        regs.irq.sp = 0x380FF80;
        regs.user.sp = 0x380FFC0;
        regs.user.lr = arm7_entry_addr;
        regs.pc = arm7_entry_addr;
        regs.set_cpsr::<false>(0x000000DF, emu);
    }

    {
        let arm9_code = common.cartridge.io.read_arm9_code();
        let arm7_code = common.cartridge.io.read_arm7_code();

        debug_println!("write ARM9 code at {:x}", arm9_ram_addr);
        for (i, value) in arm9_code.iter().enumerate() {
            emu.mem_write::<{ ARM9 }, _>(arm9_ram_addr + i as u32, *value);
        }

        debug_println!("write ARM7 code at {:x}", arm7_ram_addr);
        for (i, value) in arm7_code.iter().enumerate() {
            emu.mem_write::<{ ARM7 }, _>(arm7_ram_addr + i as u32, *value);
        }
    }

    Gpu::initialize_schedule(get_cm_mut!(emu));
    common.gpu.frame_limit = emu.settings.framelimit();
    common.gpu.gpu_renderer = Some(gpu_renderer);

    get_spu_mut!(emu).audio_enabled = emu.settings.audio();
    Spu::initialize_schedule(get_cm_mut!(emu));

    let save_thread = thread::Builder::new()
        .name("save".to_owned())
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::Low, ThreadAffinity::Core1);
            let last_save_time = last_save_time;
            let emu = unsafe { (emu_ptr as *mut Emu).as_mut().unwrap_unchecked() };
            let common = get_common_mut!(emu);
            loop {
                common.cartridge.io.flush_save_buf(&last_save_time);
                thread::sleep(Duration::from_secs(3));
            }
        })
        .unwrap();

    if emu.settings.arm7_hle() {
        common.ipc.use_hle();
        common.gpu.arm7_hle = true;
        execute_jit::<true>(&mut emu_unsafe);
    } else {
        execute_jit::<false>(&mut emu_unsafe);
    }

    save_thread.join().unwrap();
}

pub static mut JIT_ASM_ARM9_PTR: *mut JitAsm<{ ARM9 }> = ptr::null_mut();
pub static mut JIT_ASM_ARM7_PTR: *mut JitAsm<{ ARM7 }> = ptr::null_mut();

pub unsafe fn get_jit_asm_ptr<'a, const CPU: CpuType>() -> *mut JitAsm<'a, CPU> {
    match CPU {
        ARM9 => JIT_ASM_ARM9_PTR as usize as *mut JitAsm<'a, CPU>,
        ARM7 => JIT_ASM_ARM7_PTR as usize as *mut JitAsm<'a, CPU>,
    }
}

#[inline(never)]
fn execute_jit<const ARM7_HLE: bool>(emu: &mut UnsafeCell<Emu>) {
    let mut jit_asm_arm9 = JitAsm::<{ ARM9 }>::new(unsafe { emu.get().as_mut().unwrap() });
    let mut jit_asm_arm7 = JitAsm::<{ ARM7 }>::new(unsafe { emu.get().as_mut().unwrap() });

    unsafe {
        JIT_ASM_ARM9_PTR = &mut jit_asm_arm9;
        JIT_ASM_ARM7_PTR = &mut jit_asm_arm7;
    }

    let emu = emu.get_mut();
    get_jit_mut!(emu).open();

    get_mmu!(jit_asm_arm9.emu, ARM9).update_all(emu);
    get_mmu!(jit_asm_arm7.emu, ARM7).update_all(emu);

    let cpu_regs_arm9 = get_cpu_regs!(emu, ARM9);
    let cpu_regs_arm7 = get_cpu_regs!(emu, ARM7);

    let cm = &mut get_common_mut!(emu).cycle_manager;
    let gpu_3d_regs = &mut get_common_mut!(emu).gpu.gpu_3d_regs;

    loop {
        let arm9_cycles = if likely(!cpu_regs_arm9.is_halted() && !jit_asm_arm9.runtime_data.idle_loop) {
            (jit_asm_arm9.execute() + 1) >> 1
        } else {
            0
        };

        if ARM7_HLE {
            if unlikely(cpu_regs_arm9.is_halted() || jit_asm_arm9.runtime_data.idle_loop) {
                cm.jump_to_next_event();
            } else {
                cm.add_cycles(arm9_cycles);
            }
        } else {
            let arm7_cycles = if likely(!cpu_regs_arm7.is_halted() && !jit_asm_arm7.runtime_data.idle_loop) {
                jit_asm_arm7.execute()
            } else {
                0
            };

            let cycles = min(arm9_cycles.wrapping_sub(1), arm7_cycles.wrapping_sub(1)).wrapping_add(1);
            if unlikely(cycles == 0) {
                cm.jump_to_next_event();
            } else {
                cm.add_cycles(cycles);
            }
        }

        if unlikely(cm.check_events(emu)) {
            jit_asm_arm9.runtime_data.idle_loop = false;
            if !ARM7_HLE {
                jit_asm_arm7.runtime_data.idle_loop = false;
            }
        }

        gpu_3d_regs.run_cmds(cm.get_cycles(), emu);
    }
}

pub fn main() {
    // For some reason setting the stack size with the global variable doesn't work
    // #[used]
    // #[export_name = "sceUserMainThreadStackSize"]
    // pub static SCE_USER_MAIN_THREAD_STACK_SIZE: u32 = 4 * 1024 * 1024;
    // Instead just create a new thread with stack size set
    if cfg!(target_os = "vita") {
        set_thread_prio_affinity(ThreadPriority::Low, ThreadAffinity::Core0);
    }
    thread::Builder::new()
        .name("actual_main".to_string())
        .stack_size(4 * 1024 * 1024) // We reserve 2MB for jit registers
        .spawn(actual_main)
        .unwrap()
        .join()
        .unwrap();
}

// Must be pub for vita
pub fn actual_main() {
    if cfg!(target_os = "vita") {
        set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core1);
    }

    if DEBUG_LOG {
        std::env::set_var("RUST_BACKTRACE", "full");
    }

    let mut presenter = Presenter::new();
    let (cartridge_io, settings) = presenter.present_ui();
    presenter.destroy_ui();

    let fps = Arc::new(AtomicU16::new(0));
    let fps_clone = fps.clone();

    let key_map = Arc::new(AtomicU32::new(0xFFFFFFFF));
    let key_map_clone = key_map.clone();

    let touch_points = Arc::new(AtomicU16::new(0));
    let touch_points_clone = touch_points.clone();

    let sound_sampler = Arc::new(SoundSampler::new());
    let sound_sampler_clone = sound_sampler.clone();

    let presenter_audio = presenter.get_presenter_audio();
    let audio_thread = if settings.audio() {
        Some(
            thread::Builder::new()
                .name("audio".to_owned())
                .spawn(move || {
                    set_thread_prio_affinity(ThreadPriority::Default, ThreadAffinity::Core0);
                    let mut audio_buffer = HeapMemU32::<{ PRESENTER_AUDIO_BUF_SIZE }>::new();
                    loop {
                        sound_sampler.consume(audio_buffer.deref_mut());
                        presenter_audio.play(audio_buffer.deref());
                    }
                })
                .unwrap(),
        )
    } else {
        None
    };

    let gpu_renderer = UnsafeCell::new(GpuRenderer::new());
    let gpu_renderer_ptr = gpu_renderer.get() as u32;

    let last_save_time = Arc::new(Mutex::new(None));
    let last_save_time_clone = last_save_time.clone();

    let cpu_thread = thread::Builder::new()
        .name("cpu".to_owned())
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core2);
            run_cpu(
                cartridge_io,
                fps_clone,
                key_map_clone,
                touch_points_clone,
                sound_sampler_clone,
                settings,
                NonNull::new(gpu_renderer_ptr as *mut GpuRenderer).unwrap(),
                last_save_time_clone,
            );
        })
        .unwrap();

    let gpu_renderer = unsafe { gpu_renderer.get().as_mut().unwrap() };
    while let PresentEvent::Inputs { keymap, touch } = presenter.poll_event() {
        if let Some((x, y)) = touch {
            touch_points.store(((y as u16) << 8) | (x as u16), Ordering::Relaxed);
        }
        key_map.store(keymap, Ordering::Relaxed);

        gpu_renderer.render_loop(&mut presenter, &fps, &last_save_time);
    }

    if let Some(audio_thread) = audio_thread {
        audio_thread.join().unwrap();
    }
    cpu_thread.join().unwrap();
}

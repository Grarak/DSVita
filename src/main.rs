#![allow(incomplete_features)]
#![allow(internal_features)]
#![feature(adt_const_params)]
#![feature(allocator_api)]
#![feature(arm_target_feature)]
#![feature(const_trait_impl)]
#![feature(core_intrinsics)]
#![feature(downcast_unchecked)]
#![feature(generic_const_exprs)]
#![feature(ptr_as_ref_unchecked)]
#![feature(seek_stream_len)]
#![feature(slice_swap_unchecked)]
#![feature(stdarch_arm_neon_intrinsics)]
#![feature(stmt_expr_attributes)]
#![feature(vec_push_within_capacity)]

use crate::core::cycle_manager::EventType;
use crate::core::emu::Emu;
use crate::core::graphics::gl_utils::create_shader;
use crate::core::graphics::gpu::{Gpu, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::graphics::gpu_shaders::GpuShadersPrograms;
use crate::core::memory::regions;
use crate::core::spi::MicSampler;
use crate::core::spu::{SoundSampler, SAMPLE_BUFFER_SIZE};
use crate::core::thread_regs::ThreadRegs;
use crate::core::{spi, CpuType};
use crate::jit::jit_asm::{JitAsm, MAX_STACK_DEPTH_SIZE};
use crate::jit::jit_memory::JitMemory;
use crate::logging::{debug_println, info_println};
use crate::mmap::{register_abort_handler, ArmContext, Mmap, PAGE_SIZE};
use crate::presenter::ui::UiPauseMenuReturn;
use crate::presenter::{PresentEvent, Presenter, PRESENTER_AUDIO_IN_BUF_SIZE, PRESENTER_AUDIO_OUT_BUF_SIZE};
use crate::settings::Arm7Emu;
use crate::utils::{const_str_equal, set_thread_prio_affinity, start_profiling, stop_profiling, HeapArray, HeapArrayU32, ThreadAffinity, ThreadPriority};
use std::cell::UnsafeCell;
use std::cmp::min;
use std::intrinsics::unlikely;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::Thread;
use std::time::Duration;
use std::{mem, slice, thread};
use CpuType::{ARM7, ARM9};

mod bitset;
mod cartridge_io;
mod cartridge_metadata;
mod core;
mod fixed_fifo;
mod jit;
mod logging;
mod math;
mod mmap;
mod presenter;
mod screen_layouts;
mod settings;
mod soundtouch;
mod utils;

const BUILD_PROFILE_NAME: &str = include_str!(concat!(env!("OUT_DIR"), "/build_profile_name"));
pub const DEBUG_LOG: bool = const_str_equal(BUILD_PROFILE_NAME, "debug");
pub const IS_DEBUG: bool = DEBUG_LOG || const_str_equal(BUILD_PROFILE_NAME, "release-debug");
pub const BRANCH_LOG: bool = DEBUG_LOG;

fn run_cpu(emu: &mut Emu) {
    let arm9_ram_addr = emu.cartridge.io.header.arm9_values.ram_address;
    let arm9_entry_addr = emu.cartridge.io.header.arm9_values.entry_address;
    let arm7_ram_addr = emu.cartridge.io.header.arm7_values.ram_address;
    let arm7_entry_addr = emu.cartridge.io.header.arm7_values.entry_address;

    info_println!("ARM9 entry addr {arm9_entry_addr:x}");
    info_println!("ARM7 entry addr {arm7_entry_addr:x}");

    emu.reset();
    emu.cm.schedule(0x7FFFFFFF, EventType::Overflow);

    emu.mem.shm[regions::GBA_ROM_REGION.shm_offset..regions::GBA_ROM_REGION.shm_offset + regions::GBA_ROM_REGION.size].fill(0xFF);

    info_println!("Initialize mmu");
    emu.mmu_update_all::<{ ARM9 }>();
    emu.mmu_update_all::<{ ARM7 }>();

    {
        emu.cp15_write(0x010000, 0x0005707D); // control
        emu.cp15_write(0x090100, 0x0300000A); // dtcm addr/size
        emu.cp15_write(0x090101, 0x00000020); // itcm size
    }

    {
        info_println!("Copying cartridge header to main");
        let cartridge_header: &[u8; cartridge_io::HEADER_IN_RAM_SIZE] = unsafe { mem::transmute(&emu.cartridge.io.header) };
        emu.mem_write_multiple_slice::<{ ARM9 }, false, _>(0x27FFE00, cartridge_header);

        emu.mem_write_no_tcm::<{ ARM9 }, _>(0x27FF850, 0x5835u16); // ARM7 BIOS CRC
        emu.mem_write_no_tcm::<{ ARM9 }, _>(0x27FF880, 0x0007u16); // Message from ARM9 to ARM7
        emu.mem_write_no_tcm::<{ ARM9 }, _>(0x27FF884, 0x0006u16); // ARM7 boot task
        emu.mem_write_no_tcm::<{ ARM9 }, _>(0x27FFC10, 0x5835u16); // Copy of ARM7 BIOS CRC
        emu.mem_write_no_tcm::<{ ARM9 }, _>(0x27FFC40, 0x0001u16); // Boot indicator

        emu.mem_write_no_tcm::<{ ARM9 }, _>(0x27FF800, 0x00001FC2u32); // Chip ID 1
        emu.mem_write_no_tcm::<{ ARM9 }, _>(0x27FF804, 0x00001FC2u32); // Chip ID 2
        emu.mem_write_no_tcm::<{ ARM9 }, _>(0x27FFC00, 0x00001FC2u32); // Copy of chip ID 1
        emu.mem_write_no_tcm::<{ ARM9 }, _>(0x27FFC04, 0x00001FC2u32); // Copy of chip ID 2

        // User settings
        let user_settings = unsafe { slice::from_raw_parts(emu.spi.firmware.as_ptr().add(spi::USER_SETTINGS_1_ADDR), 0x70) };
        emu.mem_write_multiple_slice::<{ ARM9 }, false, _>(0x27FFC80, user_settings);
    }

    // unsafe {
    //     // Empty GBA ROM
    //     let gba_rom_ptr = mem.mmu_arm9.get_base_ptr().add(regions::GBA_ROM_OFFSET as usize);
    //     mmap::set_protection(gba_rom_ptr, regions::GBA_ROM_REGION.size, true, true, false);
    //     gba_rom_ptr.write_bytes(0xFF, regions::GBA_ROM_REGION.size);
    //     mmap::set_protection(gba_rom_ptr, regions::GBA_ROM_REGION.size, true, false, false);
    // }

    {
        // I/O Ports
        emu.mem_write::<{ ARM9 }, _>(0x4000247, 0x03u8);
        emu.mem_write::<{ ARM9 }, _>(0x4000300, 0x01u8);
        emu.mem_write::<{ ARM9 }, _>(0x4000304, 0x0001u16);
    }

    {
        let regs = ARM9.thread_regs();
        regs.user.gp_regs[4] = arm9_entry_addr; // R12
        regs.user.sp = 0x3002F7C;
        regs.irq.sp = 0x3003F80;
        regs.svc.sp = 0x3003FC0;
        regs.user.lr = arm9_entry_addr;
        regs.pc = arm9_entry_addr;
        emu.thread_set_cpsr(ARM9, 0x000000DF, false);
    }

    {
        // I/O Ports
        emu.mem_write::<{ ARM7 }, _>(0x4000300, 0x01u8); // POWCNT1
        emu.mem_write::<{ ARM7 }, _>(0x4000504, 0x0200u16); // SOUNDBIAS
    }

    {
        let regs = ARM7.thread_regs();
        regs.user.gp_regs[4] = arm7_entry_addr; // R12
        regs.user.sp = 0x380FD80;
        regs.irq.sp = 0x380FF80;
        regs.user.sp = 0x380FFC0;
        regs.user.lr = arm7_entry_addr;
        regs.pc = arm7_entry_addr;
        emu.thread_set_cpsr(ARM7, 0x000000DF, false);
    }

    {
        let arm9_code = emu.cartridge.io.read_arm9_code();
        let arm7_code = emu.cartridge.io.read_arm7_code();

        info_println!("write ARM9 code at {:x}", arm9_ram_addr);
        for (i, value) in arm9_code.iter().enumerate() {
            emu.mem_write::<{ ARM9 }, _>(arm9_ram_addr + i as u32, *value);
        }

        info_println!("write ARM7 code at {:x}", arm7_ram_addr);
        for (i, value) in arm7_code.iter().enumerate() {
            emu.mem_write::<{ ARM7 }, _>(arm7_ram_addr + i as u32, *value);
        }
    }

    Gpu::initialize_schedule(&mut emu.cm);
    emu.spu_initialize_schedule();

    if emu.settings.arm7_emu() == Arm7Emu::Hle {
        emu.arm7_hle_initialize();
    }

    unsafe { register_abort_handler(fault_handler).unwrap() };

    let jit_asm_arm9 = unsafe { (ARM9.jit_asm_addr() as *mut JitAsm).as_mut_unchecked() };
    let jit_asm_arm7 = unsafe { (ARM7.jit_asm_addr() as *mut JitAsm).as_mut_unchecked() };

    jit_asm_arm9.parse_nitrosdk_entry();

    if emu.settings.arm7_emu() == Arm7Emu::Hle {
        execute_jit::<true>(jit_asm_arm9, jit_asm_arm7);
    } else {
        execute_jit::<false>(jit_asm_arm9, jit_asm_arm7);
    }
}

pub static mut CURRENT_RUNNING_CPU: CpuType = ARM9;

pub unsafe fn get_jit_asm_ptr<'a, const CPU: CpuType>() -> *mut JitAsm<'a> {
    match CPU {
        ARM9 => CPU.jit_asm_addr() as *mut JitAsm<'a>,
        ARM7 => CPU.jit_asm_addr() as *mut JitAsm<'a>,
    }
}

unsafe fn process_fault<const CPU: CpuType>(mem_addr: usize, host_pc: &mut usize, arm_context: &ArmContext) -> bool {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut_unchecked() };

    debug_println!("{CPU:?} fault at {host_pc:x} {mem_addr:x}");
    if mem_addr < CPU.mmu_tcm_addr() {
        eprintln!("{CPU:?} fault {host_pc:x} {mem_addr:x} outside of mapped memory");
        return false;
    }

    let guest_mem_addr = (mem_addr - CPU.mmu_tcm_addr()) as u32;
    debug_println!("{CPU:?} guest fault at {host_pc:x} {mem_addr:x} to guest {guest_mem_addr:x}");
    asm.emu.jit.patch_slow_mem(host_pc, guest_mem_addr, CPU, arm_context)
}

#[cold]
fn fault_handler(mem_addr: usize, host_pc: &mut usize, arm_context: &ArmContext) -> bool {
    unsafe {
        match CURRENT_RUNNING_CPU {
            ARM9 => process_fault::<{ ARM9 }>(mem_addr, host_pc, arm_context),
            ARM7 => process_fault::<{ ARM7 }>(mem_addr, host_pc, arm_context),
        }
    }
}

#[inline(never)]
fn execute_jit<const ARM7_HLE: bool>(jit_asm_arm9: &mut JitAsm, jit_asm_arm7: &mut JitAsm) {
    loop {
        let arm9_cycles = if !jit_asm_arm9.emu.cpu_is_halted(ARM9) {
            unsafe { CURRENT_RUNNING_CPU = ARM9 };
            (jit_asm_arm9.execute::<{ ARM9 }>() + 1) >> 1
        } else {
            0
        };

        if ARM7_HLE {
            if unlikely(jit_asm_arm9.emu.cpu_is_halted(ARM9)) {
                jit_asm_arm9.emu.cm.jump_to_next_event();
            } else {
                jit_asm_arm9.emu.cm.add_cycles(arm9_cycles);
            }
        } else {
            let arm7_cycles = if !jit_asm_arm9.emu.cpu_is_halted(ARM7) && !jit_asm_arm7.runtime_data.is_idle_loop() {
                unsafe { CURRENT_RUNNING_CPU = ARM7 };
                jit_asm_arm7.execute::<{ ARM7 }>()
            } else {
                0
            };

            let cycles = min(arm9_cycles.wrapping_sub(1), arm7_cycles.wrapping_sub(1)).wrapping_add(1);
            if unlikely(cycles == 0) {
                jit_asm_arm9.emu.cm.jump_to_next_event();
            } else {
                jit_asm_arm9.emu.cm.add_cycles(cycles);
            }
        }

        if jit_asm_arm9.emu.cm_check_events() && !ARM7_HLE {
            jit_asm_arm7.runtime_data.set_idle_loop(false);
        }

        jit_asm_arm9.emu.regs_3d_run_cmds(jit_asm_arm9.emu.cm.get_cycles());

        if unlikely(jit_asm_arm9.emu.gpu.renderer.is_quit()) {
            break;
        }
    }
}

#[used]
#[export_name = "_newlib_heap_size_user"]
pub static _NEWLIB_HEAP_SIZE_USER: u32 = 256 * 1024 * 1024; // 256 MiB

pub fn main() {
    // For some reason setting the stack size with the global variable doesn't work
    // #[used]
    // #[export_name = "sceUserMainThreadStackSize"]
    // pub static SCE_USER_MAIN_THREAD_STACK_SIZE: u32 = 4 * 1024 * 1024;
    // Instead just create a new thread with stack size set
    if cfg!(target_os = "vita") {
        set_thread_prio_affinity(ThreadPriority::Low, &[ThreadAffinity::Core0]);
    }
    thread::Builder::new()
        .name("actual_main".to_string())
        .stack_size(4 * 1024 * 1024)
        .spawn(actual_main)
        .unwrap()
        .join()
        .unwrap();
}

pub fn actual_main() {
    if cfg!(target_os = "vita") {
        set_thread_prio_affinity(ThreadPriority::High, &[ThreadAffinity::Core0]);
    }

    info_println!("Starting DSVita");

    if IS_DEBUG {
        std::env::set_var("RUST_BACKTRACE", "1");
        #[cfg(target_os = "linux")]
        std::panic::set_hook(Box::new(|panic_info| {
            let mut count = 0;
            let cwd = std::env::current_dir();
            backtrace::trace(|frame| {
                backtrace::resolve_frame(frame, |symbols| {
                    eprint!("{count}: {:4x} - ", frame.ip() as usize);
                    match symbols.name() {
                        None => eprint!("<unknown>"),
                        Some(name) => {
                            eprint!("{name}");
                            if name.to_string().starts_with("dsvita") {
                                eprint!(" <----------");
                            }
                        }
                    }
                    eprintln!();
                    if let (Some(file), Some(line)) = (symbols.filename(), symbols.lineno()) {
                        eprint!("{:4}", "");
                        if let Ok(cwd) = &cwd {
                            if let Ok(suffix) = file.strip_prefix(cwd) {
                                eprint!("          at {suffix:?}:{line}");
                            } else {
                                eprint!("          at {file:?}:{line}");
                            }
                        } else {
                            eprint!("          at {file:?}:{line}");
                        }
                        if let Some(colno) = symbols.colno() {
                            eprint!(":{colno}")
                        }
                        eprintln!();
                    }
                });

                count += 1;
                count < 25
            });

            eprintln!();
            eprintln!(
                "{}: {} <----------",
                panic_info.payload_as_str().unwrap_or("No payload"),
                panic_info
                    .location()
                    .map_or("No location".to_string(), |location| { format!("{}:{}:{}", location.file(), location.line(), location.column()) })
            );
            eprintln!();
        }));

        #[cfg(target_os = "vita")]
        {
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let location = info.location().unwrap();

                let msg = match info.payload().downcast_ref::<&'static str>() {
                    Some(s) => *s,
                    None => match info.payload().downcast_ref::<String>() {
                        Some(s) => &s[..],
                        None => "Box<Any>",
                    },
                };
                info_println!("panicked at {location}: '{msg}'");

                default_hook(info);
            }));
        }
    }

    let mut presenter = Presenter::new();

    let fps = Arc::new(AtomicU16::new(0));
    let key_map = Arc::new(AtomicU32::new(0xFFFFFFFF));
    let touch_points = Arc::new(AtomicU16::new(0));
    let mic_sampler = Arc::new(Mutex::new(MicSampler::new()));
    let mut sound_sampler = UnsafeCell::new(SoundSampler::new());
    let mut gpu_renderer = None;

    let fps_clone = fps.clone();
    let key_map_clone = key_map.clone();
    let touch_points_clone = touch_points.clone();
    let mic_sampler_clone = mic_sampler.clone();
    let sound_sampler_ptr = sound_sampler.get() as usize;

    let mut arm9_thread_regs = Mmap::rw("arm9_thread_regs", ARM9.guest_regs_addr(), utils::align_up(size_of::<ThreadRegs>(), PAGE_SIZE)).unwrap();
    let mut arm7_thread_regs = Mmap::rw("arm7_thread_regs", ARM7.guest_regs_addr(), utils::align_up(size_of::<ThreadRegs>(), PAGE_SIZE)).unwrap();
    let arm9_thread_regs = arm9_thread_regs.as_mut_ptr() as *mut ThreadRegs;
    let arm7_thread_regs = arm7_thread_regs.as_mut_ptr() as *mut ThreadRegs;
    unsafe {
        *arm9_thread_regs = ThreadRegs::default();
        *arm7_thread_regs = ThreadRegs::default();
    }

    // Initializing jit mem inside of emu, breaks kubridge for some reason
    // Might be caused by initialize shared mem? Initialize here and pass it to emu
    let jit_mem = JitMemory::new();
    let mut emu_unsafe = UnsafeCell::new(Emu::new(
        fps_clone,
        key_map_clone,
        touch_points_clone,
        mic_sampler_clone,
        NonNull::from(sound_sampler.get_mut()),
        jit_mem,
    ));
    let emu_ptr = emu_unsafe.get() as usize;

    let mut jit_asm_arm9 = Mmap::rw("arm9_jit_asm", ARM9.jit_asm_addr(), utils::align_up(size_of::<JitAsm>(), PAGE_SIZE)).unwrap();
    let mut jit_asm_arm7 = Mmap::rw("arm7_jit_asm", ARM7.jit_asm_addr(), utils::align_up(size_of::<JitAsm>(), PAGE_SIZE)).unwrap();
    let jit_asm_arm9: &'static mut JitAsm = unsafe { mem::transmute(jit_asm_arm9.as_mut_ptr()) };
    let jit_asm_arm7: &'static mut JitAsm = unsafe { mem::transmute(jit_asm_arm7.as_mut_ptr()) };
    *jit_asm_arm9 = JitAsm::new(ARM9, unsafe { emu_unsafe.get().as_mut().unwrap() });
    *jit_asm_arm7 = JitAsm::new(ARM7, unsafe { emu_unsafe.get().as_mut().unwrap() });

    let cpu_active = Arc::new(AtomicBool::new(true));

    let mut running = true;
    while running {
        let (mut cartridge_io, settings) = match presenter.present_ui() {
            Some((cartridge_io, settings)) => (cartridge_io, settings),
            None => return,
        };
        info_println!("{} Settings: {settings:?}", cartridge_io.file_name);
        presenter.on_game_launched();

        if gpu_renderer.is_none() {
            let mut counter = 0;
            gpu_renderer = Some(GpuRenderer::new(&GpuShadersPrograms::new(|name, src, shader_type| unsafe {
                let name = format!(
                    "Compiling {name} {}shader",
                    match shader_type {
                        gl::VERTEX_SHADER => "vertex ",
                        gl::FRAGMENT_SHADER => "fragment ",
                        _ => "",
                    }
                );
                presenter.present_progress(&name, counter, GpuShadersPrograms::count());
                counter += 1;
                create_shader(name, src, shader_type).unwrap()
            })));
            emu_unsafe.get_mut().gpu.set_gpu_renderer(NonNull::from(gpu_renderer.as_mut().unwrap()));
        }
        emu_unsafe.get_mut().gpu.renderer.init();

        let presenter_audio_out = presenter.get_presenter_audio_out();
        let presenter_audio_in = presenter.get_presenter_audio_in();

        let last_save_time = Arc::new(Mutex::new(None));
        let last_save_time_clone = last_save_time.clone();

        cartridge_io.parse_overlays();
        info_println!("Found {} overlays", cartridge_io.overlays.len());

        emu_unsafe.get_mut().cartridge.set_cartridge_io(cartridge_io);
        emu_unsafe.get_mut().settings = settings;

        sound_sampler.get_mut().init();

        let cpu_thread = thread::Builder::new()
            .name("cpu".to_owned())
            .stack_size(MAX_STACK_DEPTH_SIZE + 1024 * 1024) // Add 1MB headroom to stack
            .spawn(move || {
                set_thread_prio_affinity(ThreadPriority::High, &[ThreadAffinity::Core2]);
                info_println!("Start cpu {:?}", thread::current().id());
                let emu = emu_ptr as *mut Emu;
                start_profiling();
                run_cpu(unsafe { emu.as_mut_unchecked() });
                stop_profiling();
                info_println!("Stopped cpu {:?}", thread::current().id());
            })
            .unwrap();

        let cpu_thread_ptr = cpu_thread.thread() as *const _ as usize;
        cpu_active.store(true, Ordering::SeqCst);

        let cpu_active_clone = cpu_active.clone();
        let vram_read_thread = thread::Builder::new()
            .name("vram_read".to_owned())
            .spawn(move || {
                set_thread_prio_affinity(ThreadPriority::High, &[ThreadAffinity::Core0, ThreadAffinity::Core1]);
                let emu = unsafe { (emu_ptr as *mut Emu).as_mut_unchecked() };
                let cpu_active = cpu_active_clone;
                let vram = emu.vram_get_mem();
                while cpu_active.load(Ordering::Relaxed) {
                    emu.gpu.renderer.read_vram(vram);
                }
                info_println!("Stopped vram read");
            })
            .unwrap();

        let cpu_active_clone = cpu_active.clone();
        let process_3d_thread = thread::Builder::new()
            .name("process_3d_thread".to_owned())
            .spawn(move || {
                set_thread_prio_affinity(ThreadPriority::Default, &[ThreadAffinity::Core1]);
                let emu = unsafe { (emu_ptr as *mut Emu).as_mut_unchecked() };
                let cpu_active = cpu_active_clone;
                while cpu_active.load(Ordering::Relaxed) {
                    emu.gpu.renderer.process_3d_loop();
                }
                info_println!("Stopped process 3d");
            })
            .unwrap();

        let cpu_active_clone = cpu_active.clone();
        let audio_out_thread = thread::Builder::new()
            .name("audio_out".to_owned())
            .spawn(move || {
                set_thread_prio_affinity(ThreadPriority::Default, &[ThreadAffinity::Core0, ThreadAffinity::Core1]);
                let mut guest_buffer = HeapArrayU32::<{ SAMPLE_BUFFER_SIZE }>::default();
                let mut audio_buffer = HeapArrayU32::<{ PRESENTER_AUDIO_OUT_BUF_SIZE }>::default();
                let emu = unsafe { (emu_ptr as *mut Emu).as_mut_unchecked() };
                let sound_sampler = unsafe { (sound_sampler_ptr as *mut SoundSampler).as_mut_unchecked() };
                let cpu_thread = unsafe { (cpu_thread_ptr as *const Thread).as_ref_unchecked() };
                let cpu_active = cpu_active_clone;
                while cpu_active.load(Ordering::Relaxed) {
                    sound_sampler.consume(cpu_thread, &mut guest_buffer, &mut audio_buffer, emu.settings.audio_stretching());
                    presenter_audio_out.play(&audio_buffer);
                }
            })
            .unwrap();

        let cpu_active_clone = cpu_active.clone();
        let mic_sampler = mic_sampler.clone();
        let audio_in_thread = thread::Builder::new()
            .name("audio_in".to_owned())
            .spawn(move || {
                set_thread_prio_affinity(ThreadPriority::Low, &[ThreadAffinity::Core0, ThreadAffinity::Core1]);
                let mut audio_buffer = HeapArray::<i16, { PRESENTER_AUDIO_IN_BUF_SIZE }>::default();
                let cpu_active = cpu_active_clone;
                while cpu_active.load(Ordering::Relaxed) {
                    presenter_audio_in.receive(&mut audio_buffer);
                    {
                        let mut mic_sampler = mic_sampler.lock().unwrap();
                        mic_sampler.push(&mut audio_buffer);
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            })
            .unwrap();

        let cpu_active_clone = cpu_active.clone();
        let save_thread = thread::Builder::new()
            .name("save".to_owned())
            .spawn(move || {
                set_thread_prio_affinity(ThreadPriority::Low, &[ThreadAffinity::Core0, ThreadAffinity::Core1]);
                let last_save_time = last_save_time_clone;
                let emu = unsafe { (emu_ptr as *mut Emu).as_mut().unwrap_unchecked() };
                let cpu_active = cpu_active_clone;
                'outer: loop {
                    for _ in 0..6 {
                        if !cpu_active.load(Ordering::Relaxed) {
                            break 'outer;
                        }
                        thread::sleep(Duration::from_millis(500));
                    }
                    emu.cartridge.io.flush_save_buf(&last_save_time);
                }
            })
            .unwrap();

        let gpu_renderer = gpu_renderer.as_mut().unwrap();
        let mut screen_layout = emu_unsafe.get_mut().settings.screen_layout();
        let arm7_emu = emu_unsafe.get_mut().settings.arm7_emu();
        loop {
            let pause = match presenter.poll_event(&emu_unsafe.get_mut().settings) {
                PresentEvent::Inputs { mut keymap, touch } => {
                    if let Some((x, y)) = touch {
                        let (x_norm, y_norm) = screen_layout.normalize_touch_points(x, y);
                        if x_norm >= 0 && x_norm < DISPLAY_WIDTH as i16 && y_norm >= 0 && y_norm < DISPLAY_HEIGHT as i16 {
                            touch_points.store(((y_norm as u16) << 8) | (x_norm as u16), Ordering::Relaxed);
                            keymap &= !(1 << 16);
                        }
                    }
                    key_map.store(keymap, Ordering::Relaxed);
                    false
                }
                PresentEvent::CycleScreenLayout {
                    offset,
                    swap,
                    top_screen_scale_offset,
                    bottom_screen_scale_offset,
                } => {
                    screen_layout = screen_layout.apply_settings_event(offset, swap, top_screen_scale_offset, bottom_screen_scale_offset);
                    false
                }
                PresentEvent::Pause => true,
                PresentEvent::Quit => {
                    running = false;
                    true
                }
            };

            gpu_renderer.render_loop(&mut presenter, &fps, &last_save_time, arm7_emu, &screen_layout, pause);

            if unlikely(!running) {
                gpu_renderer.set_quit(true);
                gpu_renderer.unpause(cpu_thread.thread());
                break;
            } else if unlikely(pause) {
                emu_unsafe.get_mut().settings.set_screen_layout(&screen_layout);
                match presenter.present_pause(gpu_renderer, &mut emu_unsafe.get_mut().settings) {
                    UiPauseMenuReturn::Resume => {
                        screen_layout = emu_unsafe.get_mut().settings.screen_layout();
                        gpu_renderer.unpause(cpu_thread.thread());
                    }
                    UiPauseMenuReturn::Quit => {
                        gpu_renderer.set_quit(true);
                        gpu_renderer.unpause(cpu_thread.thread());
                        break;
                    }
                    UiPauseMenuReturn::QuitApp => {
                        running = false;
                        gpu_renderer.set_quit(true);
                        gpu_renderer.unpause(cpu_thread.thread());
                        break;
                    }
                }
            }
        }

        cpu_thread.join().unwrap();
        cpu_active.store(false, Ordering::SeqCst);
        audio_out_thread.join().unwrap();
        audio_in_thread.join().unwrap();
        vram_read_thread.join().unwrap();
        process_3d_thread.join().unwrap();
        save_thread.join().unwrap();
        gpu_renderer.set_quit(false);
    }
}

#![allow(incomplete_features)]
#![allow(internal_features)]
#![feature(adt_const_params)]
#![feature(allocator_api)]
#![feature(arm_target_feature)]
#![feature(const_trait_impl)]
#![feature(core_intrinsics)]
#![feature(downcast_unchecked)]
#![feature(generic_const_exprs)]
#![feature(new_zeroed_alloc)]
#![feature(panic_payload_as_str)]
#![feature(ptr_as_ref_unchecked)]
#![feature(seek_stream_len)]
#![feature(slice_swap_unchecked)]
#![feature(stdarch_arm_neon_intrinsics)]
#![feature(stmt_expr_attributes)]
#![feature(vec_push_within_capacity)]

use crate::cartridge_io::CartridgeIo;
use crate::core::emu::Emu;
use crate::core::graphics::gpu::Gpu;
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::spu::SoundSampler;
use crate::core::thread_regs::ThreadRegs;
use crate::core::{spi, CpuType};
use crate::jit::jit_asm::{JitAsm, MAX_STACK_DEPTH_SIZE};
use crate::jit::jit_memory::JitMemory;
use crate::logging::debug_println;
use crate::mmap::{register_abort_handler, ArmContext, Mmap, PAGE_SIZE};
use crate::presenter::{PresentEvent, Presenter, PRESENTER_AUDIO_BUF_SIZE};
use crate::settings::{Arm7Emu, Settings};
use crate::utils::{const_str_equal, set_thread_prio_affinity, HeapMemU32, ThreadAffinity, ThreadPriority};
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

mod bitset;
mod cartridge_io;
mod cartridge_metadata;
mod core;
mod fixed_fifo;
mod jit;
mod linked_list;
mod logging;
mod math;
mod mmap;
mod presenter;
mod settings;
mod utils;

const BUILD_PROFILE_NAME: &str = include_str!(concat!(env!("OUT_DIR"), "/build_profile_name"));
pub const DEBUG_LOG: bool = const_str_equal(BUILD_PROFILE_NAME, "debug");
pub const IS_DEBUG: bool = !const_str_equal(BUILD_PROFILE_NAME, "release");
pub const BRANCH_LOG: bool = DEBUG_LOG;

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

    let mut arm9_thread_regs = Mmap::rw("arm9_thread_regs", ARM9.guest_regs_addr(), utils::align_up(size_of::<ThreadRegs>(), PAGE_SIZE)).unwrap();
    let mut arm7_thread_regs = Mmap::rw("arm7_thread_regs", ARM7.guest_regs_addr(), utils::align_up(size_of::<ThreadRegs>(), PAGE_SIZE)).unwrap();
    let arm9_thread_regs: &'static mut ThreadRegs = unsafe { mem::transmute(arm9_thread_regs.as_mut_ptr()) };
    let arm7_thread_regs: &'static mut ThreadRegs = unsafe { mem::transmute(arm7_thread_regs.as_mut_ptr()) };
    *arm9_thread_regs = ThreadRegs::new();
    *arm7_thread_regs = ThreadRegs::new();

    // Initializing jit mem inside of emu, breaks kubridge for some reason
    // Might be caused by initialize shared mem? Initialize here and pass it to emu
    let jit_mem = JitMemory::new(&settings);
    let mut emu_unsafe = UnsafeCell::new(Emu::new(
        [arm9_thread_regs, arm7_thread_regs],
        cartridge_io,
        fps,
        key_map,
        touch_points,
        sound_sampler,
        jit_mem,
        settings,
    ));
    let emu_ptr = emu_unsafe.get() as u32;
    let emu = emu_unsafe.get_mut();

    debug_println!("Initialize mmu");
    emu.mmu_update_all::<{ ARM9 }>();
    emu.mmu_update_all::<{ ARM7 }>();

    {
        emu.cp15_write(0x010000, 0x0005707D); // control
        emu.cp15_write(0x090100, 0x0300000A); // dtcm addr/size
        emu.cp15_write(0x090101, 0x00000020); // itcm size
    }

    {
        debug_println!("Copying cartridge header to main");
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
        let user_settings = &spi::SPI_FIRMWARE[spi::USER_SETTINGS_1_ADDR..spi::USER_SETTINGS_1_ADDR + 0x70];
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
        let regs = &mut emu.thread[ARM9];
        regs.user.gp_regs[4] = arm9_entry_addr; // R12
        regs.user.sp = 0x3002F7C;
        regs.irq.sp = 0x3003F80;
        regs.svc.sp = 0x3003FC0;
        regs.user.lr = arm9_entry_addr;
        regs.pc = arm9_entry_addr;
        emu.thread_set_cpsr::<false>(ARM9, 0x000000DF);
    }

    {
        // I/O Ports
        emu.mem_write::<{ ARM7 }, _>(0x4000300, 0x01u8); // POWCNT1
        emu.mem_write::<{ ARM7 }, _>(0x4000504, 0x0200u16); // SOUNDBIAS
    }

    {
        let regs = &mut emu.thread[ARM7];
        regs.user.gp_regs[4] = arm7_entry_addr; // R12
        regs.user.sp = 0x380FD80;
        regs.irq.sp = 0x380FF80;
        regs.user.sp = 0x380FFC0;
        regs.user.lr = arm7_entry_addr;
        regs.pc = arm7_entry_addr;
        emu.thread_set_cpsr::<false>(ARM7, 0x000000DF);
    }

    {
        let arm9_code = emu.cartridge.io.read_arm9_code();
        let arm7_code = emu.cartridge.io.read_arm7_code();

        debug_println!("write ARM9 code at {:x}", arm9_ram_addr);
        for (i, value) in arm9_code.iter().enumerate() {
            emu.mem_write::<{ ARM9 }, _>(arm9_ram_addr + i as u32, *value);
        }

        debug_println!("write ARM7 code at {:x}", arm7_ram_addr);
        for (i, value) in arm7_code.iter().enumerate() {
            emu.mem_write::<{ ARM7 }, _>(arm7_ram_addr + i as u32, *value);
        }
    }

    Gpu::initialize_schedule(&mut emu.cm);
    emu.gpu.gpu_renderer = Some(gpu_renderer);

    emu.spu.audio_enabled = emu.settings.audio();
    emu.spu_initialize_schedule();

    if emu.settings.arm7_hle() == Arm7Emu::Hle {
        emu.arm7_hle_initialize();
    }

    let save_thread = thread::Builder::new()
        .name("save".to_owned())
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::Low, ThreadAffinity::Core1);
            let last_save_time = last_save_time;
            let emu = unsafe { (emu_ptr as *mut Emu).as_mut().unwrap_unchecked() };
            loop {
                emu.cartridge.io.flush_save_buf(&last_save_time);
                thread::sleep(Duration::from_secs(3));
            }
        })
        .unwrap();

    unsafe { register_abort_handler(fault_handler).unwrap() };

    if emu.settings.arm7_hle() == Arm7Emu::Hle {
        execute_jit::<true>(&mut emu_unsafe);
    } else {
        execute_jit::<false>(&mut emu_unsafe);
    }

    save_thread.join().unwrap();
}

pub static mut JIT_ASM_ARM9_PTR: *mut JitAsm<{ ARM9 }> = ptr::null_mut();
pub static mut JIT_ASM_ARM7_PTR: *mut JitAsm<{ ARM7 }> = ptr::null_mut();
pub static mut CURRENT_RUNNING_CPU: CpuType = ARM9;

pub unsafe fn get_jit_asm_ptr<'a, const CPU: CpuType>() -> *mut JitAsm<'a, CPU> {
    match CPU {
        ARM9 => JIT_ASM_ARM9_PTR as usize as *mut JitAsm<'a, CPU>,
        ARM7 => JIT_ASM_ARM7_PTR as usize as *mut JitAsm<'a, CPU>,
    }
}

unsafe fn process_fault<const CPU: CpuType>(mem_addr: usize, host_pc: &mut usize, arm_context: &ArmContext) -> bool {
    let asm = unsafe { get_jit_asm_ptr::<CPU>().as_mut_unchecked() };

    debug_println!("fault at {host_pc:x} {mem_addr:x}");
    if mem_addr < CPU.mmu_tcm_addr() {
        return false;
    }

    let guest_mem_addr = (mem_addr - CPU.mmu_tcm_addr()) as u32;
    debug_println!("guest fault at {host_pc:x} {mem_addr:x} to guest {guest_mem_addr:x}");
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
fn execute_jit<const ARM7_HLE: bool>(emu: &mut UnsafeCell<Emu>) {
    let mut jit_asm_arm9 = JitAsm::<{ ARM9 }>::new(unsafe { emu.get().as_mut().unwrap() });
    let mut jit_asm_arm7 = JitAsm::<{ ARM7 }>::new(unsafe { emu.get().as_mut().unwrap() });

    let emu = emu.get_mut();

    jit_asm_arm9.init_common_funs();
    jit_asm_arm7.init_common_funs();

    unsafe {
        JIT_ASM_ARM9_PTR = &mut jit_asm_arm9;
        JIT_ASM_ARM7_PTR = &mut jit_asm_arm7;
    }

    loop {
        let arm9_cycles = if likely(!emu.cpu_is_halted(ARM9)) {
            unsafe { CURRENT_RUNNING_CPU = ARM9 };
            (jit_asm_arm9.execute() + 1) >> 1
        } else {
            0
        };

        if ARM7_HLE {
            if unlikely(emu.cpu_is_halted(ARM9)) {
                emu.cm.jump_to_next_event();
            } else {
                emu.cm.add_cycles(arm9_cycles);
            }
        } else {
            let arm7_cycles = if likely(!emu.cpu_is_halted(ARM7) && !jit_asm_arm7.runtime_data.is_idle_loop()) {
                unsafe { CURRENT_RUNNING_CPU = ARM7 };
                jit_asm_arm7.execute()
            } else {
                0
            };

            let cycles = min(arm9_cycles.wrapping_sub(1), arm7_cycles.wrapping_sub(1)).wrapping_add(1);
            if unlikely(cycles == 0) {
                emu.cm.jump_to_next_event();
            } else {
                emu.cm.add_cycles(cycles);
            }
        }

        if emu.cm_check_events() && !ARM7_HLE {
            jit_asm_arm7.runtime_data.set_idle_loop(false);
        }

        emu.regs_3d_run_cmds(emu.cm.get_cycles());
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
        set_thread_prio_affinity(ThreadPriority::Low, ThreadAffinity::Core0);
    }
    thread::Builder::new()
        .name("actual_main".to_string())
        .stack_size(4 * 1024 * 1024)
        .spawn(actual_main)
        .unwrap()
        .join()
        .unwrap();
}

#[cold]
pub fn actual_main() {
    if cfg!(target_os = "vita") {
        set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core1);
    }

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
    }

    let mut presenter = Presenter::new();
    let (cartridge_io, settings) = presenter.present_ui();
    presenter.destroy_ui();
    eprintln!("{} Settings: {settings:?}", cartridge_io.file_name);

    let fps = Arc::new(AtomicU16::new(0));
    let fps_clone = fps.clone();

    let key_map = Arc::new(AtomicU32::new(0xFFFFFFFF));
    let key_map_clone = key_map.clone();

    let touch_points = Arc::new(AtomicU16::new(0));
    let touch_points_clone = touch_points.clone();

    let sound_sampler = Arc::new(SoundSampler::new(settings.framelimit()));
    let sound_sampler_clone = sound_sampler.clone();

    let presenter_audio = presenter.get_presenter_audio();
    let audio_thread = thread::Builder::new()
        .name("audio".to_owned())
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::Default, ThreadAffinity::Core0);
            let mut audio_buffer = HeapMemU32::<{ PRESENTER_AUDIO_BUF_SIZE }>::new();
            loop {
                sound_sampler.consume(audio_buffer.deref_mut());
                presenter_audio.play(audio_buffer.deref());
            }
        })
        .unwrap();

    let gpu_renderer = UnsafeCell::new(GpuRenderer::new());
    let gpu_renderer_ptr = gpu_renderer.get() as u32;

    let last_save_time = Arc::new(Mutex::new(None));
    let last_save_time_clone = last_save_time.clone();

    let settings_clone = settings.clone();

    let cpu_thread = thread::Builder::new()
        .name("cpu".to_owned())
        .stack_size(MAX_STACK_DEPTH_SIZE + 1024 * 1024) // Add 1MB headroom to stack
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core2);
            println!("Start cpu {:?}", thread::current().id());
            run_cpu(
                cartridge_io,
                fps_clone,
                key_map_clone,
                touch_points_clone,
                sound_sampler_clone,
                settings_clone,
                NonNull::new(gpu_renderer_ptr as *mut GpuRenderer).unwrap(),
                last_save_time_clone,
            );
        })
        .unwrap();

    let gpu_renderer = unsafe { gpu_renderer.get().as_mut().unwrap() };
    while let PresentEvent::Inputs { keymap, touch } = presenter.poll_event(settings.screenmode()) {
        if let Some((x, y)) = touch {
            touch_points.store(((y as u16) << 8) | (x as u16), Ordering::Relaxed);
        }
        key_map.store(keymap, Ordering::Relaxed);

        gpu_renderer.render_loop(&mut presenter, &fps, &last_save_time, &settings);
    }

    audio_thread.join().unwrap();
    cpu_thread.join().unwrap();
}

#![allow(incomplete_features)]
#![feature(adt_const_params)]
#![feature(allocator_api)]
#![feature(arm_target_feature)]
#![feature(const_trait_impl)]
#![feature(core_intrinsics)]
#![feature(generic_const_exprs)]
#![feature(isqrt)]
#![feature(naked_functions)]
#![feature(new_uninit)]
#![feature(seek_stream_len)]
#![feature(stmt_expr_attributes)]

extern crate core;

use crate::cartridge_reader::CartridgeReader;
use crate::emu::emu::{
    get_cm_mut, get_common_mut, get_cp15_mut, get_cpu_regs, get_jit_mut, get_mem, get_mem_mut,
    get_mmu, get_regs_mut, Emu,
};
use crate::emu::gpu::gpu::{Gpu, Swapchain, DISPLAY_PIXEL_COUNT};
use crate::emu::spu::{SoundSampler, Spu};
use crate::emu::{spi, CpuType};
use crate::jit::jit_asm::JitAsm;
use crate::logging::debug_println;
use crate::presenter::{PresentEvent, Presenter, PRESENTER_AUDIO_BUF_SIZE};
use crate::settings::{
    create_settings_mut, Settings, ARM7_HLE_SETTINGS, AUDIO_SETTING, FRAMESKIP_SETTING,
};
use crate::utils::{set_thread_prio_affinity, HeapMemU32, ThreadAffinity, ThreadPriority};
use std::cell::{RefCell, UnsafeCell};
use std::cmp::min;
use std::intrinsics::{likely, unlikely};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use std::sync::Arc;
use std::{mem, thread};
use CpuType::{ARM7, ARM9};

mod cartridge_reader;
mod emu;
mod jit;
mod logging;
mod mmap;
mod presenter;
mod settings;
mod utils;

pub const DEBUG_LOG: bool = cfg!(debug_assertions);
pub const DEBUG_LOG_BRANCH_OUT: bool = DEBUG_LOG;

fn run_cpu(
    cartridge_reader: CartridgeReader,
    swapchain: Arc<Swapchain>,
    fps: Arc<AtomicU16>,
    key_map: Arc<AtomicU32>,
    touch_points: Arc<AtomicU16>,
    sound_sampler: Arc<SoundSampler>,
    settings: Arc<Settings>,
) {
    let arm9_ram_addr = cartridge_reader.header.arm9_values.ram_address;
    let arm9_entry_addr = cartridge_reader.header.arm9_values.entry_address;
    let arm7_ram_addr = cartridge_reader.header.arm7_values.ram_address;
    let arm7_entry_addr = cartridge_reader.header.arm7_values.entry_address;

    let mut emu_unsafe = UnsafeCell::new(Emu::new(
        cartridge_reader,
        swapchain,
        fps,
        key_map,
        touch_points,
        sound_sampler,
    ));
    let emu = emu_unsafe.get_mut();
    let common = get_common_mut!(emu);
    let mem = get_mem_mut!(emu);

    {
        let cartridge_header: &[u8; cartridge_reader::HEADER_IN_RAM_SIZE] =
            unsafe { mem::transmute(&common.cartridge.reader.header) };
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
        mem.main.write_slice(
            0x27FFC80,
            &spi::SPI_FIRMWARE[spi::USER_SETTINGS_1_ADDR..spi::USER_SETTINGS_1_ADDR + 0x70],
        );
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
        regs.set_cpsr::<false>(0x000000DF, get_cm_mut!(emu));
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
        regs.set_cpsr::<false>(0x000000DF, get_cm_mut!(emu));
    }

    {
        let arm9_code = common.cartridge.reader.read_arm9_code();
        let arm7_code = common.cartridge.reader.read_arm7_code();

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
    common.gpu.frame_skip = settings[FRAMESKIP_SETTING].value.as_bool().unwrap();

    if settings[AUDIO_SETTING].value.as_bool().unwrap() {
        Spu::initialize_schedule(get_cm_mut!(emu));
    }

    let emu_ptr = emu_unsafe.get() as u32;
    let gpu2d_thread = thread::Builder::new()
        .name("gpu2d".to_owned())
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core1);
            let emu = unsafe { (emu_ptr as *mut Emu).as_mut().unwrap() };
            let common = get_common_mut!(emu);
            loop {
                common.gpu.draw_scanline_thread(get_mem!(emu));
            }
        })
        .unwrap();

    if settings[ARM7_HLE_SETTINGS].value.as_bool().unwrap() {
        common.ipc.use_hle();
        common.gpu.arm7_hle = true;
        execute_jit::<true>(&mut emu_unsafe);
    } else {
        execute_jit::<false>(&mut emu_unsafe);
    }
    gpu2d_thread.join().unwrap();
}

#[inline(never)]
fn execute_jit<const ARM7_HLE: bool>(emu: &mut UnsafeCell<Emu>) {
    let mut jit_asm_arm9 = JitAsm::<{ ARM9 }>::new(unsafe { emu.get().as_mut().unwrap() });
    let mut jit_asm_arm7 = JitAsm::<{ ARM7 }>::new(unsafe { emu.get().as_mut().unwrap() });

    let emu = emu.get_mut();
    get_jit_mut!(emu).open();

    get_mmu!(jit_asm_arm9.emu, ARM9).update_all(emu);
    get_mmu!(jit_asm_arm7.emu, ARM7).update_all(emu);

    let cpu_regs_arm9 = get_cpu_regs!(emu, ARM9);
    let cpu_regs_arm7 = get_cpu_regs!(emu, ARM7);

    let cm = &mut get_common_mut!(emu).cycle_manager;

    loop {
        let arm9_cycles = if likely(!cpu_regs_arm9.is_halted()) {
            (jit_asm_arm9.execute() + 1) >> 1
        } else {
            0
        };

        if ARM7_HLE {
            if unlikely(arm9_cycles == 0) {
                cm.jump_to_next_event();
            } else {
                cm.add_cycle(arm9_cycles);
            }
        } else {
            let arm7_cycles = if likely(!cpu_regs_arm7.is_halted()) {
                jit_asm_arm7.execute()
            } else {
                0
            };

            let cycles =
                min(arm9_cycles.wrapping_sub(1), arm7_cycles.wrapping_sub(1)).wrapping_add(1);
            if unlikely(cycles == 0) {
                cm.jump_to_next_event();
            } else {
                cm.add_cycle(cycles);
            }
        }

        cm.check_events(jit_asm_arm9.emu);
    }
}

// Must be pub for vita
pub fn main() {
    set_thread_prio_affinity(ThreadPriority::Low, ThreadAffinity::Core0);

    if DEBUG_LOG {
        std::env::set_var("RUST_BACKTRACE", "full");
    }

    let settings_mut = Rc::new(RefCell::new(create_settings_mut()));
    let file_path = Rc::new(RefCell::new(PathBuf::new()));

    let mut presenter = Presenter::new();

    #[cfg(target_os = "vita")]
    {
        use crate::presenter::menu::{Menu, MenuAction, MenuPresenter};
        use std::fs;

        let root_menu = Menu::new(
            "DSPSV",
            vec![
                Menu::new("Settings", Vec::new(), |menu| {
                    menu.entries = settings_mut
                        .borrow()
                        .iter()
                        .enumerate()
                        .map(|(index, setting)| {
                            let settings_clone = settings_mut.clone();
                            Menu::new(
                                format!("{} - {}", setting.title, setting.value),
                                Vec::new(),
                                move |_| {
                                    settings_clone.borrow_mut()[index].value.next();
                                    MenuAction::Refresh
                                },
                            )
                        })
                        .collect();
                    MenuAction::EnterSubMenu
                }),
                Menu::new("Select ROM", Vec::new(), |menu| {
                    match fs::read_dir("ux0:dspsv") {
                        Ok(dirs) => {
                            menu.entries = dirs
                                .into_iter()
                                .filter_map(|dir| {
                                    dir.ok().and_then(|dir| {
                                        dir.file_type().ok().and_then(|file_type| {
                                            if file_type.is_file() {
                                                Some(dir)
                                            } else {
                                                None
                                            }
                                        })
                                    })
                                })
                                .map(|dir| {
                                    let file_path_clone = file_path.clone();
                                    Menu::new(dir.path().to_str().unwrap(), Vec::new(), move |_| {
                                        *file_path_clone.borrow_mut() = dir.path();
                                        MenuAction::Quit
                                    })
                                })
                                .collect();
                            if menu.entries.is_empty() {
                                menu.entries.push(Menu::new(
                                    "ux0:dspsv does not contain any files!",
                                    Vec::new(),
                                    |_| MenuAction::Refresh,
                                ))
                            }
                        }
                        Err(_) => {
                            menu.entries.clear();
                            menu.entries.push(Menu::new(
                                "ux0:dspsv does not exist!",
                                Vec::new(),
                                |_| MenuAction::Refresh,
                            ));
                        }
                    }
                    MenuAction::EnterSubMenu
                }),
            ],
            |_| MenuAction::EnterSubMenu,
        );
        let mut menu_presenter = MenuPresenter::new(&mut presenter, root_menu);
        menu_presenter.present();
    }

    #[cfg(target_os = "linux")]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.len() == 2 {
            *file_path.borrow_mut() = args[1].clone().into()
        } else {
            eprintln!("Usage {} <path_to_nds>", args[0]);
            std::process::exit(1);
        }
    }

    let cartridge_reader =
        CartridgeReader::from_file(file_path.borrow().to_str().unwrap()).unwrap();
    drop(file_path);

    let swapchain = Arc::new(Swapchain::new());
    let swapchain_clone = swapchain.clone();
    let fps = Arc::new(AtomicU16::new(0));
    let fps_clone = fps.clone();

    let key_map = Arc::new(AtomicU32::new(0xFFFFFFFF));
    let key_map_clone = key_map.clone();

    let touch_points = Arc::new(AtomicU16::new(0));
    let touch_points_clone = touch_points.clone();

    let sound_sampler = Arc::new(SoundSampler::new());
    let sound_sampler_clone = sound_sampler.clone();

    let settings = Arc::new(settings_mut.borrow().clone());
    drop(settings_mut);

    let audio_enabled = settings[AUDIO_SETTING].value.as_bool().unwrap();

    let cpu_thread = thread::Builder::new()
        .name("cpu".to_owned())
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core2);
            run_cpu(
                cartridge_reader,
                swapchain_clone,
                fps_clone,
                key_map_clone,
                touch_points_clone,
                sound_sampler_clone,
                settings,
            );
        })
        .unwrap();

    let presenter_audio = presenter.get_presenter_audio();

    let audio_thread = if audio_enabled {
        Some(
            thread::Builder::new()
                .name("audio".to_owned())
                .spawn(move || {
                    set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core0);
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

    while let PresentEvent::Inputs { keymap, touch } = presenter.event_poll() {
        if let Some((x, y)) = touch {
            touch_points.store(((y as u16) << 8) | (x as u16), Ordering::Relaxed);
        }
        key_map.store(keymap, Ordering::Relaxed);

        let fb = swapchain.consume();
        let top = unsafe {
            (fb[..DISPLAY_PIXEL_COUNT].as_ptr() as *const [u32; DISPLAY_PIXEL_COUNT])
                .as_ref()
                .unwrap_unchecked()
        };
        let bottom = unsafe {
            (fb[DISPLAY_PIXEL_COUNT..].as_ptr() as *const [u32; DISPLAY_PIXEL_COUNT])
                .as_ref()
                .unwrap_unchecked()
        };
        presenter.present_textures(top, bottom, fps.load(Ordering::Relaxed));
    }

    cpu_thread.join().unwrap();
    if let Some(audio_thread) = audio_thread {
        audio_thread.join().unwrap();
    }
}

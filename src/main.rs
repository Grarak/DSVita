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
use crate::hle::gpu::gpu::{Gpu, Swapchain, DISPLAY_PIXEL_COUNT};
use crate::hle::hle::{get_cm, get_cp15_mut, get_cpu_regs, get_mmu, get_regs_mut, Hle};
use crate::hle::{spi, CpuType};
use crate::jit::jit_asm::JitAsm;
use crate::logging::debug_println;
use crate::presenter::{PresentEvent, Presenter};
use crate::settings::{create_settings_mut, Settings, FRAMESKIP_SETTING};
use crate::utils::{set_thread_prio_affinity, ThreadAffinity, ThreadPriority};
use std::cell::RefCell;
use std::cmp::min;
use std::intrinsics::{likely, unlikely};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::{mem, ptr, thread};
use CpuType::{ARM7, ARM9};
mod cartridge_reader;
mod hle;
mod jit;
mod logging;
mod mmap;
mod presenter;
mod settings;
mod utils;

pub const DEBUG_LOG: bool = cfg!(debug_assertions);

fn run_cpu(
    cartridge_reader: CartridgeReader,
    swapchain: Arc<Swapchain>,
    fps: Arc<AtomicU16>,
    key_map: Arc<AtomicU16>,
    settings: Arc<Settings>,
) {
    let arm9_ram_addr = cartridge_reader.header.arm9_values.ram_address;
    let arm9_entry_addr = cartridge_reader.header.arm9_values.entry_address;
    let arm7_ram_addr = cartridge_reader.header.arm7_values.ram_address;
    let arm7_entry_addr = cartridge_reader.header.arm7_values.entry_address;

    let mut hle = Hle::new(cartridge_reader, swapchain, fps, key_map);

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

        debug_println!("write ARM9 code at {:x}", arm9_ram_addr);
        for (i, value) in arm9_code.iter().enumerate() {
            hle.mem_write::<{ ARM9 }, _>(arm9_ram_addr + i as u32, *value);
        }

        debug_println!("write ARM7 code at {:x}", arm7_ram_addr);
        for (i, value) in arm7_code.iter().enumerate() {
            hle.mem_write::<{ ARM7 }, _>(arm7_ram_addr + i as u32, *value);
        }
    }

    Gpu::initialize_schedule(get_cm!(hle));
    hle.common.gpu.frame_skip = settings[FRAMESKIP_SETTING].value.as_bool().unwrap();

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

    let key_map = Arc::new(AtomicU16::new(0xFFFF));
    let key_map_clone = key_map.clone();

    let settings = Arc::new(settings_mut.borrow().clone());
    drop(settings_mut);

    let cpu_thread = thread::Builder::new()
        .name("cpu".to_owned())
        .spawn(move || {
            set_thread_prio_affinity(ThreadPriority::High, ThreadAffinity::Core2);
            run_cpu(
                cartridge_reader,
                swapchain_clone,
                fps_clone,
                key_map_clone,
                settings,
            );
        })
        .unwrap();

    while let PresentEvent::Keymap(value) = presenter.event_poll() {
        key_map.store(value, Ordering::Relaxed);

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
}

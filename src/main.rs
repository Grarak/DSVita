#![allow(incomplete_features)]
#![feature(adt_const_params)]
#![feature(arm_target_feature)]
#![feature(const_trait_impl)]
#![feature(thread_id_value)]
#![feature(unchecked_math)]
#![feature(unchecked_shifts)]

use crate::cartridge::Cartridge;
use crate::hle::cp15_context::Cp15Context;
use crate::hle::cpu_regs::CpuRegs;
use crate::hle::cycle_manager::CycleManager;
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::input_context::InputContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::main_memory::MainMemory;
use crate::hle::memory::oam_context::OamContext;
use crate::hle::memory::palettes_context::PalettesContext;
use crate::hle::memory::regions;
use crate::hle::memory::tcm_context::TcmContext;
use crate::hle::memory::vram_context::VramContext;
use crate::hle::memory::wram_context::WramContext;
use crate::hle::rtc_context::RtcContext;
use crate::hle::spi_context;
use crate::hle::spi_context::SpiContext;
use crate::hle::spu_context::SpuContext;
use crate::hle::thread_context::ThreadContext;
use crate::hle::CpuType;
use crate::jit::jit_memory::JitMemory;
use crate::scheduler::IO_SCHEDULER;
use crate::utils::FastCell;
use std::rc::Rc;
use std::sync::{mpsc, Mutex};
use std::sync::{Arc, RwLock};
use std::{env, mem, thread};

mod cartridge;
mod hle;
mod jit;
mod logging;
mod mmap;
mod scheduler;
mod utils;

pub const DEBUG: bool = cfg!(debug_assertions);
// pub const DEBUG: bool = false;

#[cfg(target_os = "linux")]
fn get_file_path() -> String {
    let args: Vec<String> = env::args().collect();
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
        regs.set_cpsr(0x000000DF);
    }
}

fn initialize_arm7_thread(entry_addr: u32, thread: &mut ThreadContext<{ CpuType::ARM7 }>) {
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
        regs.set_cpsr(0x000000DF);
    }
}

// Must be pub for vita
pub fn main() {
    if DEBUG {
        env::set_var("RUST_BACKTRACE", "full");
    }

    let (tx, rx) = mpsc::channel::<()>();
    IO_SCHEDULER.schedule(move || {
        tx.send(()).unwrap();
    });

    let cartridge = Cartridge::from_file(&get_file_path()).unwrap();

    let arm9_ram_addr = cartridge.header.arm9_values.ram_address;
    let arm9_entry_adrr = cartridge.header.arm9_values.entry_address;
    let arm7_ram_addr = cartridge.header.arm7_values.ram_address;
    let arm7_entry_addr = cartridge.header.arm7_values.entry_address;

    assert_eq!(arm9_ram_addr, regions::MAIN_MEMORY_OFFSET);

    let main_memory = Arc::new(RwLock::new(MainMemory::new()));
    {
        let memory = &mut main_memory.write().unwrap();

        let header: &[u8; cartridge::HEADER_IN_RAM_SIZE] =
            unsafe { mem::transmute(&cartridge.header) };
        memory.write_main_slice(0x27FFE00, header);

        memory.write_main(0x27FF850, 0x5835u16); // ARM7 BIOS CRC
        memory.write_main(0x27FF880, 0x0007u16); // Message from ARM9 to ARM7
        memory.write_main(0x27FF884, 0x0006u16); // ARM7 boot task
        memory.write_main(0x27FFC10, 0x5835u16); // Copy of ARM7 BIOS CRC
        memory.write_main(0x27FFC40, 0x0001u16); // Boot indicator

        memory.write_main(0x27FF800, 0x00001FC2u32); // Chip ID 1
        memory.write_main(0x27FF804, 0x00001FC2u32); // Chip ID 2
        memory.write_main(0x27FFC00, 0x00001FC2u32); // Copy of chip ID 1
        memory.write_main(0x27FFC04, 0x00001FC2u32); // Copy of chip ID 2

        // User settings
        memory.write_main_slice(
            0x27FFC80,
            &spi_context::SPI_FIRMWARE
                [spi_context::USER_SETTINGS_1_ADDR..spi_context::USER_SETTINGS_1_ADDR + 0x70],
        );

        let arm9_code = cartridge.read_arm9_code().unwrap();
        memory.write_main_slice(arm9_ram_addr, &arm9_code);

        let arm7_code = cartridge.read_arm7_code().unwrap();
        memory.write_main_slice(arm7_ram_addr, &arm7_code);
    }

    let cycle_manager = Arc::new(CycleManager::new());
    let cpu_regs_arm9 = Arc::new(CpuRegs::<{ CpuType::ARM9 }>::new());
    let cpu_regs_arm7 = Arc::new(CpuRegs::<{ CpuType::ARM7 }>::new());
    let gpu_2d_context_a = Rc::new(FastCell::new(Gpu2DContext::new()));
    let gpu_2d_context_b = Rc::new(FastCell::new(Gpu2DContext::new()));
    let gpu_context = Arc::new(RwLock::new(GpuContext::new(
        cycle_manager.clone(),
        gpu_2d_context_a.clone(),
        gpu_2d_context_b.clone(),
    )));
    let jit_memory = Arc::new(Mutex::new(JitMemory::new()));
    let wram_context = Arc::new(WramContext::new());
    let spi_context = Arc::new(RwLock::new(SpiContext::new()));
    let ipc_handler = Arc::new(RwLock::new(IpcHandler::new()));
    let vram_context = Arc::new(VramContext::new());
    let input_context = Arc::new(RwLock::new(InputContext::new()));
    let rtc_context = Rc::new(FastCell::new(RtcContext::new()));
    let spu_context = Rc::new(FastCell::new(SpuContext::new()));
    let palettes_context = Rc::new(FastCell::new(PalettesContext::new()));
    let cp15_context = Rc::new(FastCell::new(Cp15Context::new()));
    let tcm_context = Rc::new(FastCell::new(TcmContext::new()));
    let oam_context = Rc::new(FastCell::new(OamContext::new()));

    let mut arm9_thread = ThreadContext::<{ CpuType::ARM9 }>::new(
        cycle_manager.clone(),
        jit_memory.clone(),
        main_memory.clone(),
        wram_context.clone(),
        spi_context.clone(),
        ipc_handler.clone(),
        vram_context.clone(),
        input_context.clone(),
        gpu_context.clone(),
        gpu_2d_context_a.clone(),
        gpu_2d_context_b.clone(),
        rtc_context.clone(),
        spu_context.clone(),
        palettes_context.clone(),
        cpu_regs_arm9,
        cp15_context.clone(),
        tcm_context.clone(),
        oam_context.clone(),
    );
    initialize_arm9_thread(arm9_entry_adrr, &mut arm9_thread);

    let mut arm7_thread = ThreadContext::<{ CpuType::ARM7 }>::new(
        cycle_manager,
        jit_memory,
        main_memory,
        wram_context,
        spi_context,
        ipc_handler,
        vram_context,
        input_context,
        gpu_context,
        gpu_2d_context_a,
        gpu_2d_context_b,
        rtc_context,
        spu_context,
        palettes_context,
        cpu_regs_arm7,
        cp15_context,
        tcm_context,
        oam_context,
    );
    initialize_arm7_thread(arm7_entry_addr, &mut arm7_thread);

    rx.recv().unwrap();

    let arm9_thread = thread::Builder::new()
        .name("arm9_thread".to_owned())
        .spawn(move || {
            arm9_thread.run();
        })
        .unwrap();

    let arm7_thread = thread::Builder::new()
        .name("arm7_thread".to_owned())
        .spawn(move || {
            arm7_thread.run();
        })
        .unwrap();

    arm9_thread.join().ok();
    arm7_thread.join().ok();
}

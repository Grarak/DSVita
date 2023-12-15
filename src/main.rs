#![feature(arm_target_feature)]
#![feature(thread_id_value)]
#![feature(unchecked_math)]
#![feature(unchecked_shifts)]

use crate::cartridge::Cartridge;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::memory::Memory;
use crate::hle::memory::regions;
use crate::hle::thread_context::ThreadContext;
use crate::hle::CpuType;
use crate::host_memory::VmManager;
use crate::jit::jit_memory::JitMemory;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::{env, mem, thread};

mod cartridge;
mod hle;
mod host_memory;
mod jit;
mod logging;
mod mmap;
mod utils;

pub const DEBUG: bool = cfg!(debug_assertions);
// pub const DEBUG: bool = false;
pub const SINGLE_CORE: bool = false;

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

fn initialize_arm9_thread(
    entry_addr: u32,
    jit_memory: Arc<RwLock<JitMemory>>,
    memory: Arc<RwLock<Memory>>,
    ipc_handler: Arc<RwLock<IpcHandler>>,
) -> ThreadContext {
    let thread = ThreadContext::new(CpuType::ARM9, jit_memory, memory, ipc_handler);

    {
        let mut cp15 = thread.cp15_context.borrow_mut();
        cp15.write(0x010000, 0x0005707D); // control
        cp15.write(0x090100, 0x0300000A); // dtcm addr/size
        cp15.write(0x090101, 0x00000020); // itcm size
    }

    {
        // I/O Ports
        let mut indirect_mem_handler = thread.indirect_mem_handler.borrow_mut();
        indirect_mem_handler.write(0x4000247, 0x03u8);
        indirect_mem_handler.write(0x4000300, 0x01u8);
        indirect_mem_handler.write(0x4000304, 0x0001u16);
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

    thread
}

fn initialize_arm7_thread(
    entry_addr: u32,
    jit_memory: Arc<RwLock<JitMemory>>,
    memory: Arc<RwLock<Memory>>,
    ipc_handler: Arc<RwLock<IpcHandler>>,
) -> ThreadContext {
    let thread = ThreadContext::new(CpuType::ARM7, jit_memory, memory, ipc_handler);

    {
        // I/O Ports
        let mut indirect_mem_handler = thread.indirect_mem_handler.borrow_mut();
        indirect_mem_handler.write(0x4000300, 0x01u8); // POWCNT1
        indirect_mem_handler.write(0x4000504, 0x0200u16); // SOUNDBIAS
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

    thread
}

// Must be pub for vita
pub fn main() {
    if DEBUG {
        env::set_var("RUST_BACKTRACE", "full");
    }

    let cartridge = Cartridge::from_file(&get_file_path()).unwrap();

    let mut vmm = VmManager::new("vm", &regions::ARM9_REGIONS).unwrap();
    println!("Allocate vm at {:x}", vmm.vm.as_ptr() as u32);

    let mut vmmap = vmm.get_vm_mapping_mut();

    {
        let header: &[u8; cartridge::HEADER_IN_RAM_SIZE] =
            unsafe { mem::transmute(&cartridge.header) };
        vmmap[0x27FFE00..0x27FFE00 + cartridge::HEADER_IN_RAM_SIZE].copy_from_slice(header);

        let (_, aligned, _) = unsafe { vmmap.align_to_mut::<u16>() };
        aligned[0x27FF850 / 2] = 0x5835; // ARM7 BIOS CRC
        aligned[0x27FF880 / 2] = 0x0007; // Message from ARM9 to ARM7
        aligned[0x27FF884 / 2] = 0x0006; // ARM7 boot task
        aligned[0x27FFC10 / 2] = 0x5835; // Copy of ARM7 BIOS CRC
        aligned[0x27FFC40 / 2] = 0x0001; // Boot indicator

        let (_, aligned, _) = unsafe { vmmap.align_to_mut::<u32>() };
        aligned[0x27FF800 / 4] = 0x00001FC2; // Chip ID 1
        aligned[0x27FF804 / 4] = 0x00001FC2; // Chip ID 2
        aligned[0x27FFC00 / 4] = 0x00001FC2; // Copy of chip ID 1
        aligned[0x27FFC04 / 4] = 0x00001FC2; // Copy of chip ID 2
    }

    let arm9_ram_addr = cartridge.header.arm9_values.ram_address;
    let arm9_entry_adrr = cartridge.header.arm9_values.entry_address;
    let arm7_ram_addr = cartridge.header.arm7_values.ram_address;
    let arm7_entry_addr = cartridge.header.arm7_values.entry_address;

    assert_eq!(arm9_ram_addr, regions::MAIN_MEMORY_OFFSET);

    {
        let arm9_code = cartridge.read_arm9_code().unwrap();
        vmmap[arm9_ram_addr as usize..arm9_ram_addr as usize + arm9_code.len()]
            .copy_from_slice(&arm9_code);

        let arm7_code = cartridge.read_arm7_code().unwrap();
        vmmap[arm7_ram_addr as usize..arm7_ram_addr as usize + arm7_code.len()]
            .copy_from_slice(&arm7_code);
    }

    let jit_memory = Arc::new(RwLock::new(JitMemory::new()));
    let memory = Arc::new(RwLock::new(Memory::new(vmm)));
    let ipc_handler = Arc::new(RwLock::new(IpcHandler::new()));

    let jit_memory_clone = jit_memory.clone();
    let memory_clone = memory.clone();
    let ipc_handler_clone = ipc_handler.clone();

    if SINGLE_CORE {
        let mut arm9_thread = initialize_arm9_thread(
            arm9_entry_adrr,
            jit_memory_clone,
            memory_clone,
            ipc_handler_clone,
        );

        let mut arm7_thread =
            initialize_arm7_thread(arm7_entry_addr, jit_memory, memory, ipc_handler);

        loop {
            arm9_thread.iterate(2);
            arm7_thread.iterate(1);
        }
    } else {
        let (tx, rx) = mpsc::channel::<()>();

        let arm9_thread = thread::Builder::new()
            .name("arm9_thread".to_owned())
            .spawn(move || {
                let mut arm9_thread = initialize_arm9_thread(
                    arm9_entry_adrr,
                    jit_memory_clone,
                    memory_clone,
                    ipc_handler_clone,
                );

                tx.send(()).unwrap();
                arm9_thread.run();
            })
            .unwrap();

        let arm7_thread = thread::Builder::new()
            .name("arm7_thread".to_owned())
            .spawn(move || {
                rx.recv().unwrap();

                let mut arm7_thread =
                    initialize_arm7_thread(arm7_entry_addr, jit_memory, memory, ipc_handler);
                arm7_thread.run();
            })
            .unwrap();

        arm9_thread.join().unwrap();
        arm7_thread.join().unwrap();
    }
}

#![feature(unchecked_math)]
#![feature(unchecked_shifts)]

use crate::cartridge::Cartridge;
use crate::cpu::thread_context::ThreadContext;
use crate::memory::{VmManager, ARM7_REGIONS, ARM9_REGIONS};
use std::env;

// #[macro_use]
// extern crate static_assertions;

mod cartridge;
mod cpu;
mod jit;
mod logging;
mod memory;
mod mmap;
mod utils;

pub const DEBUG: bool = cfg!(debug_assertions);

const FILE_PATH: &str = "ux0:hello_world.nds";

#[cfg(target_os = "linux")]
fn get_file_path() -> String {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 {
        args[1].clone()
    } else {
        println!("Usage {} <path_to_nds>", args[0]);
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn get_file_path() -> String {
    FILE_PATH.to_owned()
}

// Must be pub for vita
pub fn main() {
    if DEBUG {
        env::set_var("RUST_BACKTRACE", "full");
    }

    let cartridge = Cartridge::from_file(&get_file_path()).unwrap();
    let arm9_ram_addr = cartridge.header.arm9_values.ram_address;
    let arm9_entry_adrr = cartridge.header.arm9_values.entry_address;

    assert_eq!(arm9_ram_addr, memory::MAIN_MEMORY_REGION.offset);

    let mut arm9_vmm = VmManager::new("arm9_vm", &ARM9_REGIONS).unwrap();
    println!("Allocate arm9 vm at {:x}", arm9_vmm.vm.as_ptr() as u32);

    let mut arm7_vmm = VmManager::new("arm7_vm", &ARM7_REGIONS).unwrap();
    println!("Allocate arm7 vm at {:x}", arm7_vmm.vm.as_ptr() as u32);

    let arm9_boot_code = cartridge.read_arm9_boot_code().unwrap();
    arm9_vmm.vm[..arm9_boot_code.len()].copy_from_slice(&arm9_boot_code);

    let mut arm9_thread = ThreadContext::new(arm9_vmm);
    {
        let mut regs = arm9_thread.regs.borrow_mut();
        regs.gp_regs[12] = arm9_entry_adrr;
        regs.sp = 0x3002F7C;
        regs.lr = arm9_entry_adrr;
        regs.pc = arm9_entry_adrr;
    }
    arm9_thread.run();
}

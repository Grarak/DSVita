#![feature(unchecked_math)]
#![feature(unchecked_shifts)]

use crate::cartridge::Cartridge;
use crate::hle::thread_context::ThreadContext;
use crate::memory::{VmManager, ARM7_REGIONS, ARM9_REGIONS};
use std::env;

// #[macro_use]
// extern crate static_assertions;

mod cartridge;
mod hle;
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

    {
        let mut vmmap = arm9_vmm.get_vm_mapping();
        vmmap[0x4000247] = 0x03; // WRAMCNT
        vmmap[0x4000300] = 0x01; // POSTFLG (ARM9)
        vmmap[0x4000304] = 0x0001; // POWCNT1

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

    let mut arm9_thread = ThreadContext::new(arm9_vmm);
    {
        let mut cp15 = arm9_thread.cp15_context.borrow_mut();
        cp15.write(0x010000, 0x0005707D); // control
        cp15.write(0x090100, 0x0300000A); // dtcm addr/size
        cp15.write(0x090101, 0x00000020); // itcm size
    }

    {
        let mut regs = arm9_thread.regs.borrow_mut();
        regs.gp_regs[12] = arm9_entry_adrr;
        regs.sp = 0x3002F7C;
        regs.lr = arm9_entry_adrr;
        regs.pc = arm9_entry_adrr;
        regs.cpsr = 0x000000DF;
    }
    arm9_thread.run();
}

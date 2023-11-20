use crate::cartridge::Cartridge;
use crate::jit::disassembler::lookup_table::LOOKUP_TABLE;
use crate::jit::jit::JitAsm;
use libc::printf;
use std::arch::asm;
use std::env;

mod cartridge;
mod jit;
mod memory;
mod mmap;
mod utils;

extern "C" fn start_exe(entry: *const u8, stack: *const u8) {
    let entry_addr = entry as u32;
    let stack_addr = stack as u32;

    unsafe {
        asm!(
            "stmfd sp!,{{r4-r12,lr}}",
            "mov r12, {entry}",
            "mov lr, {entry}",
            "mov sp, {stack}",
            "mov r0, #0",
            "mov r1, #0",
            "mov r2, #0",
            "mov r3, #0",
            "mov r4, #0",
            "mov r5, #0",
            "mov r6, #0",
            "mov r7, #0",
            "mov r8, #0",
            "mov r9, #0",
            "mov r10, #0",
            "mov r11, #0",
            "bx lr",
            "ldmfd sp!,{{r4-r12,lr}}",
            entry = in(reg) entry_addr,
            stack = in(reg) stack_addr,
        )
    };
}

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
    env::set_var("RUST_BACKTRACE", "full");

    let cartridge = Cartridge::from_file(&get_file_path()).unwrap();
    let arm9_ram_addr = cartridge.header.arm9_values.ram_address;
    let arm9_entry_adrr = cartridge.header.arm9_values.entry_address;

    assert_eq!(arm9_ram_addr, memory::MAIN_MEMORY_REGION.offset);

    let mut memory_layout = memory::allocate_memory_layout().unwrap();

    let arm9_boot_code = cartridge.read_arm9_boot_code().unwrap();
    memory_layout[..arm9_boot_code.len()].copy_from_slice(&arm9_boot_code);

    let mut jit_asm = JitAsm::new();

    let (_, insts, _) =
        unsafe { memory_layout[(arm9_entry_adrr - arm9_ram_addr) as usize..].align_to::<u32>() };
    for inst in &insts[..20] {
        let (name, func) = &LOOKUP_TABLE[(((inst >> 16) & 0xFF0) | ((inst >> 4) & 0xF)) as usize];
        println!("Executing {}", name);
        let inst = func(&mut jit_asm, name, *inst);
        println!("{:?}", inst);
    }
}

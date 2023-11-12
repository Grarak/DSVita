use crate::cartridge::Cartridge;

mod cartridge;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        println!("Usage: {} <path to nds>", args[0]);
        std::process::exit(1);
    }

    let cartridge = Cartridge::from_file(&args[1]).unwrap();
    let title = unsafe { std::str::from_utf8_unchecked(&cartridge.header.game_title) };
    let arm9_rom_offset = cartridge.header.arm9_values.rom_offset;
    println!("{} {:x}", title, arm9_rom_offset);
}

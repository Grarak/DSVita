use std::fs::File;
use std::io::Read;
use std::mem::size_of;
use std::{io, mem};

#[repr(C)]
pub struct ArmValues {
    pub rom_offset: u32,
    pub entry_address: u32,
    pub ram_address: u32,
    pub size: u32,
}

pub struct ArmOverlay {
    overlay_offset: u32,
    overlay_size: u32,
}

#[repr(C, packed)]
pub struct CartridgeHeader {
    pub game_title: [u8; 12],
    game_code: [u8; 4],
    marker_code: [u8; 2],
    unit_code: u8,
    encryption_seed_select: u8,
    device_capacity: u8,
    reserved1: [u8; 7],
    reserved2: u8,
    nds_region: u8,
    rom_version: u8,
    autostart: u8,
    pub arm9_values: ArmValues,
    pub arm7_values: ArmValues,
    file_name_table_offset: u32,
    file_name_table_size: u32,
    file_allocation_table_offset: u32,
    file_allocation_table_size: u32,
    arm9_overlay: ArmOverlay,
    arm7_overlay: ArmOverlay,
    port_setting_normal_commands: u32,
    port_setting_key1_commands: u32,
    icon_title_offset: u32,
    secure_area_checksum: u16,
    secure_area_delay: u16,
    arm9_auto_load_list_hook_ram_address: u32,
    arm7_auto_load_list_hook_ram_address: u32,
    secure_area_disable: [u8; 8],
    total_used_rom_size: u32,
    rom_header_size: u32,
    unknown: u32,
    nand_end_rom_area: u16,
    nand_start_of_rw_area: u16,
    reserved3: [u8; 0x18],
    reserved4: [u8; 0x10],
    nintendo_logo: [u8; 0x9C],
    nintendo_logo_checksum: u16,
    header_checksum: u16,
    debug_rom_offset: u32,
    debug_size: u32,
    debug_ram_address: u32,
    reserved5: [u8; 4],
    reserved6: [u8; 0x90],
    reserved7: [u8; 0xE00],
}

const HEADER_SIZE: usize = size_of::<CartridgeHeader>();

pub struct Cartridge {
    pub header: CartridgeHeader,
}

impl Cartridge {
    pub fn new(mut file: File) -> io::Result<Self> {
        let mut raw_header = [0u8; HEADER_SIZE];
        file.read_exact(&mut raw_header)?;
        let header: CartridgeHeader = unsafe { mem::transmute(raw_header) };
        Ok(Cartridge { header })
    }

    pub fn from_file(file_path: &String) -> io::Result<Self> {
        let file = File::open(file_path)?;
        Self::new(file)
    }
}

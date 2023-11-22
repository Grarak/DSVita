use std::fs::File;
use std::io::Read;
use std::num::Wrapping;
use std::os::unix::fs::FileExt;
use std::{io, mem};

#[repr(C, packed)]
pub struct ArmValues {
    pub rom_offset: u32,
    pub entry_address: u32,
    pub ram_address: u32,
    pub size: u32,
}

#[repr(C, packed)]
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

const HEADER_SIZE: usize = mem::size_of::<CartridgeHeader>();

pub struct Cartridge {
    file: File,
    pub header: CartridgeHeader,
}

impl Cartridge {
    pub fn new(mut file: File) -> io::Result<Self> {
        let mut raw_header = [0u8; HEADER_SIZE];
        file.read_exact(&mut raw_header)?;
        let header: CartridgeHeader = unsafe { mem::transmute(raw_header) };
        Ok(Cartridge { file, header })
    }

    pub fn from_file(file_path: &str) -> io::Result<Self> {
        let file = File::open(file_path)?;
        Self::new(file)
    }

    pub fn read_arm9_boot_code(&self) -> io::Result<Vec<u8>> {
        let mut boot_code = vec![0u8; self.header.arm9_values.size as usize];
        self.file
            .read_exact_at(&mut boot_code, self.header.arm9_values.rom_offset as u64)?;

        if (0x4000..0x8000).contains(&(self.header.arm9_values.rom_offset as i32)) {
            let (_, boot_code_aligned, _) = unsafe { boot_code.align_to_mut::<u32>() };
            let (_, game_code_aligned, _) = unsafe { self.header.game_code.align_to::<u32>() };
            let id_code = game_code_aligned[0];

            {
                let key1 = Key1::new(id_code, 2, 2);
                key1.decrypt(&mut boot_code_aligned[..2]);
            }

            {
                let key1 = Key1::new(id_code, 3, 2);
                for i in (0..0x200).step_by(2) {
                    key1.decrypt(&mut boot_code_aligned[i..i + 2]);
                }
            }
        }

        Ok(boot_code)
    }
}

const KEY1_BUF_SIZE: usize = 0x412;

struct Key1 {
    key_buf: [u32; KEY1_BUF_SIZE],
}

impl Key1 {
    #[inline]
    fn crypt<I: IntoIterator<Item = usize>>(
        &self,
        data: &mut [u32],
        iter: I,
        x_end_index: usize,
        y_end_index: usize,
    ) {
        debug_assert_eq!(data.len(), 2);

        let mut y = data[0];
        let mut x = data[1];
        for i in iter {
            let z = [self.key_buf[i] ^ x];
            let (_, z_aligned, _) = unsafe { z.align_to::<u8>() };
            x = self.key_buf[0x12 + z_aligned[3] as usize];
            x = unsafe { x.unchecked_add(self.key_buf[0x112 + z_aligned[2] as usize]) };
            x ^= self.key_buf[0x212 + z_aligned[1] as usize];
            x = unsafe { x.unchecked_add(self.key_buf[0x312 + z_aligned[0] as usize]) };
            x ^= y;
            y = z[0];
        }
        data[0] = x ^ self.key_buf[x_end_index];
        data[1] = y ^ self.key_buf[y_end_index];
    }

    fn encrypt(&self, data: &mut [u32]) {
        self.crypt(data, 0..=0xF, 0x10, 0x11);
    }

    fn decrypt(&self, data: &mut [u32]) {
        self.crypt(data, (0x02..=0x11).rev(), 0x1, 0x0);
    }

    fn apply_keycode(&mut self, keycode: &mut [u32; 3], modulo: u32) {
        self.encrypt(&mut keycode[1..3]);
        self.encrypt(&mut keycode[..2]);

        let mut scratch = [0u32; 2];
        for i in 0..=0x11 {
            self.key_buf[i] ^= u32::from_be(keycode[i % modulo as usize]);
        }
        for i in (0..=0x410).step_by(2) {
            self.encrypt(&mut scratch);
            self.key_buf[i] = scratch[1];
            self.key_buf[i + 1] = scratch[0];
        }
    }

    fn new(id_code: u32, level: u8, modulo: u32) -> Self {
        let mut instance =
            Key1 {
                key_buf: [0u32; KEY1_BUF_SIZE],
            };

        let mut keycode = [id_code, id_code / 2, id_code * 2];
        if level >= 1 {
            instance.apply_keycode(&mut keycode, modulo);
        }
        if level >= 2 {
            instance.apply_keycode(&mut keycode, modulo);
        }
        keycode[1] = (Wrapping(keycode[1]) * Wrapping(2)).0;
        keycode[2] /= 2;
        if level >= 3 {
            instance.apply_keycode(&mut keycode, modulo);
        }
        instance
    }
}

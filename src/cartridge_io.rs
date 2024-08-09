use crate::cartridge_metadata::get_cartridge_metadata;
use crate::logging::debug_println;
use crate::utils::NoHashMap;
use static_assertions::const_assert_eq;
use std::cell::RefCell;
use std::cmp::min;
use std::fs::File;
use std::hint::unreachable_unchecked;
use std::io::{Read, Seek};
use std::ops::{Deref, DerefMut};
use std::os::unix::fs::FileExt;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::{io, mem, thread};

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
    pub game_code: [u8; 4],
    marker_code: [u8; 2],
    unit_code: u8,
    encryption_seed_select: u8,
    device_capacity: u8,
    reserved: [u8; 7],
    reserved1: u8,
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
    reserve2: [u8; 0x24],
    reserved3: [u8; 0x10],
    nintendo_logo: [u8; 0x9C],
    nintendo_logo_checksum: u16,
    header_checksum: u16,
    debug_rom_offset: u32,
    debug_size: u32,
    debug_ram_address: u32,
    reserved4: [u8; 4],
    reserved5: [u8; 0x90],
}

const PAGE_SIZE: u32 = 4096;

const HEADER_SIZE: usize = size_of::<CartridgeHeader>();
pub const HEADER_IN_RAM_SIZE: usize = 0x170;
const_assert_eq!(HEADER_SIZE, HEADER_IN_RAM_SIZE + 0x90);

const SAVE_SIZES: [u32; 9] = [0x000200, 0x002000, 0x008000, 0x010000, 0x020000, 0x040000, 0x080000, 0x100000, 0x800000];

pub struct CartridgeIo {
    file: File,
    pub file_size: u32,
    pub header: CartridgeHeader,
    content_pages: RefCell<NoHashMap<Rc<[u8; PAGE_SIZE as usize]>>>,
    save_file_path: String,
    save_file: Option<File>,
    pub save_file_size: u32,
    save_buf: RwLock<(Vec<u8>, bool)>,
}

unsafe impl Send for CartridgeIo {}

impl CartridgeIo {
    fn new(mut file: File, save_file_path: String, mut save_file: Option<File>) -> io::Result<Self> {
        let mut raw_header = [0u8; HEADER_SIZE];
        file.read_exact(&mut raw_header)?;
        let file_size = file.stream_len().unwrap() as u32;
        let mut save_buf = Vec::new();

        let header: CartridgeHeader = unsafe { mem::transmute(raw_header) };

        let game_code = u32::from_le_bytes(header.game_code);

        let mut save_file_size = match &mut save_file {
            None => 0,
            Some(file) => {
                let save_file_size = file.stream_len().unwrap();
                save_buf.resize(save_file_size as usize, 0u8);
                match file.read_at(&mut save_buf, 0) {
                    Ok(_) => save_file_size as u32,
                    Err(_) => {
                        save_buf.clear();
                        0
                    }
                }
            }
        };

        if save_file_size == 0 {
            if let Some(metadata) = get_cartridge_metadata(game_code) {
                save_buf.resize(metadata.save_size as usize, 0xFF);
                save_file_size = metadata.save_size;
            }
        }

        if !SAVE_SIZES.contains(&save_file_size) {
            save_file_size = 0;
        }

        Ok(CartridgeIo {
            file,
            file_size,
            header,
            content_pages: RefCell::new(NoHashMap::default()),
            save_file_path,
            save_file,
            save_file_size,
            save_buf: RwLock::new((save_buf, false)),
        })
    }

    pub fn from_file(file_path: &str, save_file: impl Into<String>) -> io::Result<Self> {
        let file = File::open(file_path)?;
        let save_file_path = save_file.into();
        let save_file = File::open(&save_file_path).ok();
        Self::new(file, save_file_path, save_file)
    }

    fn get_page(&self, page_addr: u32) -> Rc<[u8; PAGE_SIZE as usize]> {
        debug_assert_eq!(page_addr & (PAGE_SIZE - 1), 0);
        let mut pages = self.content_pages.borrow_mut();
        match pages.get(&page_addr) {
            None => {
                // exceeds 4MB
                if pages.len() >= 1024 {
                    debug_println!("clear cartridge pages");
                    pages.clear();
                }

                let mut buf = [0u8; PAGE_SIZE as usize];
                self.file.read_at(&mut buf, page_addr as u64).unwrap();
                let buf = Rc::new(buf);
                pages.insert(page_addr, buf.clone());
                buf
            }
            Some(page) => page.clone(),
        }
    }

    pub fn read_slice(&self, offset: u32, slice: &mut [u8]) {
        let mut remaining = slice.len();
        while remaining > 0 {
            let slice_start = slice.len() - remaining;

            let page_addr = (offset + slice_start as u32) & !(PAGE_SIZE - 1);
            let page_offset = offset + slice_start as u32 - page_addr;
            let page = self.get_page(page_addr);
            let page_slice = &page.as_slice()[page_offset as usize..];

            let read_amount = min(remaining, page_slice.len());
            let slice_end = slice_start + read_amount;
            slice[slice_start..slice_end].copy_from_slice(&page_slice[..read_amount]);
            remaining -= read_amount;
        }
    }

    pub fn read_arm9_code(&self) -> Vec<u8> {
        let mut boot_code = vec![0u8; self.header.arm9_values.size as usize];
        self.read_slice(self.header.arm9_values.rom_offset, &mut boot_code);

        // if (0x4000..0x8000).contains(&(self.header.arm9_values.rom_offset as i32)) {
        //     let (_, boot_code_aligned, _) = unsafe { boot_code.align_to_mut::<u32>() };
        //     let (_, game_code_aligned, _) = unsafe { self.header.game_code.align_to::<u32>() };
        //     let id_code = game_code_aligned[0];
        //
        //     {
        //         let key1 = Key1::new(id_code, 2, 2);
        //         key1.decrypt((&mut boot_code_aligned[..2]).try_into().unwrap());
        //     }
        //
        //     {
        //         let key1 = Key1::new(id_code, 3, 2);
        //         for i in (0..0x200).step_by(2) {
        //             key1.decrypt((&mut boot_code_aligned[i..i + 2]).try_into().unwrap());
        //         }
        //     }
        // }

        boot_code
    }

    pub fn read_arm7_code(&self) -> Vec<u8> {
        let mut boot_code = vec![0u8; self.header.arm7_values.size as usize];
        self.read_slice(self.header.arm7_values.rom_offset, &mut boot_code);
        boot_code
    }

    pub fn resize_save_file(&mut self, new_size: u32) {
        let mut lock = self.save_buf.write().unwrap();
        let (save_buf, _) = lock.deref_mut();
        save_buf.resize(new_size as usize, 0xFF);
        self.save_file_size = new_size;
    }

    pub fn read_save_buf(&self, addr: u32) -> u8 {
        let lock = self.save_buf.read().unwrap();
        let (save_buf, _) = lock.deref();
        save_buf[addr as usize]
    }

    pub fn write_save_buf(&self, addr: u32, value: u8) {
        let mut lock = self.save_buf.write().unwrap();
        let (save_buf, dirty) = lock.deref_mut();
        save_buf[addr as usize] = value;
        *dirty = true;
    }

    pub fn write_save_buf_slice(&self, addr: u32, buf: &[u8]) {
        let mut lock = self.save_buf.write().unwrap();
        let (save_buf, dirty) = lock.deref_mut();
        let write_len = min(buf.len(), save_buf.len());
        save_buf[addr as usize..addr as usize + write_len].copy_from_slice(&buf[..write_len]);
        *dirty = true;
    }

    pub fn flush_save_buf(&mut self, last_save_time: &Arc<RwLock<Option<Instant>>>) {
        let mut lock = self.save_buf.write().unwrap();
        let (save_buf, dirty) = lock.deref_mut();
        if *dirty {
            let file = match &self.save_file {
                None => {
                    match File::create_new(&self.save_file_path) {
                        Ok(file) => {
                            self.save_file = Some(file);
                            // On vita we have to add some delay between creation and writing to file
                            // Otherwise it gets stuck for some reason
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => return,
                    }
                    match &self.save_file {
                        None => unsafe { unreachable_unchecked() },
                        Some(file) => file,
                    }
                }
                Some(file) => file,
            };
            let _ = file.write_at(save_buf, 0);
            eprintln!("writting to save file");
            *last_save_time.write().unwrap() = Some(Instant::now());
            *dirty = false;
        }
    }
}

const KEY1_BUF_SIZE: usize = 0x412;

pub struct Key1 {
    key_buf: [u32; KEY1_BUF_SIZE],
}

impl Key1 {
    fn crypt(&self, data: &mut [u32; 2], iter: impl IntoIterator<Item = usize>, x_end_index: usize, y_end_index: usize) {
        let mut y = data[0];
        let mut x = data[1];
        for i in iter {
            let z = [self.key_buf[i] ^ x];
            let (_, z_aligned, _) = unsafe { z.align_to::<u8>() };
            x = self.key_buf[0x12 + z_aligned[3] as usize];
            x = x.wrapping_add(self.key_buf[0x112 + z_aligned[2] as usize]);
            x ^= self.key_buf[0x212 + z_aligned[1] as usize];
            x = x.wrapping_add(self.key_buf[0x312 + z_aligned[0] as usize]);
            x ^= y;
            y = z[0];
        }
        data[0] = x ^ self.key_buf[x_end_index];
        data[1] = y ^ self.key_buf[y_end_index];
    }

    fn encrypt(&self, data: &mut [u32; 2]) {
        self.crypt(data, 0..0x10, 0x10, 0x11);
    }

    fn decrypt(&self, data: &mut [u32; 2]) {
        self.crypt(data, (0x02..0x12).rev(), 0x1, 0x0);
    }

    fn apply_keycode(&mut self, keycode: &mut [u32; 3], modulo: u32) {
        self.encrypt((&mut keycode[1..3]).try_into().unwrap());
        self.encrypt((&mut keycode[..2]).try_into().unwrap());

        let mut scratch = [0u32; 2];
        for i in 0..0x12 {
            self.key_buf[i] ^= u32::from_be(keycode[i % modulo as usize]);
        }
        for i in (0..0x411).step_by(2) {
            self.encrypt(&mut scratch);
            self.key_buf[i] = scratch[1];
            self.key_buf[i + 1] = scratch[0];
        }
    }

    fn new(id_code: u32, level: u8, modulo: u32) -> Self {
        let mut instance = Key1 { key_buf: [0u32; KEY1_BUF_SIZE] };

        let mut keycode = [id_code, id_code >> 1, id_code << 1];
        if level >= 1 {
            instance.apply_keycode(&mut keycode, modulo);
        }
        if level >= 2 {
            instance.apply_keycode(&mut keycode, modulo);
        }
        keycode[1] = keycode[1].wrapping_mul(2);
        keycode[2] >>= 1;
        if level >= 3 {
            instance.apply_keycode(&mut keycode, modulo);
        }
        instance
    }
}

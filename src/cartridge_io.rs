use crate::cartridge_metadata::get_cartridge_metadata;
use crate::logging::debug_println;
use crate::utils;
use crate::utils::{rgb5_to_rgb8, HeapArrayU8, NoHashMap};
use static_assertions::const_assert_eq;
use std::cmp::min;
use std::fs::File;
use std::io::{ErrorKind, Seek};
use std::ops::{Deref, DerefMut};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
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
    game_title: [u8; 12],
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
    pub icon_title_offset: u32,
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

const HEADER_SIZE: usize = size_of::<CartridgeHeader>();
pub const HEADER_IN_RAM_SIZE: usize = 0x170;
const_assert_eq!(HEADER_SIZE, HEADER_IN_RAM_SIZE + 0x90);

#[derive(PartialEq, Eq)]
#[repr(C)]
pub struct FsOverlayInfoHeader {
    pub id: u32,
    pub ram_address: u32,
    pub ram_size: u32,
    pub bss_size: u32,
    pub sinit_init: u32,
    pub sinit_init_end: u32,
    pub file_id: u32,
    pub compressed_flag: u32,
}

impl FsOverlayInfoHeader {
    pub fn is_in_range(&self, addr: u32) -> bool {
        (self.ram_address..self.ram_address_end()).contains(&addr)
    }

    pub fn ram_address_end(&self) -> u32 {
        self.ram_address + self.ram_size
    }

    pub fn address_end(&self) -> u32 {
        self.ram_address + self.total_size()
    }

    pub fn total_size(&self) -> u32 {
        self.ram_size + self.bss_size
    }
}

const SAVE_SIZES: [u32; 9] = [0x000200, 0x002000, 0x008000, 0x010000, 0x020000, 0x040000, 0x080000, 0x100000, 0x800000];
const CARTRIDGE_PAGE_SIZE: usize = 4096;
const MAX_CARTRIDGE_CACHE: usize = 16 * 1024 * 1024;

pub struct CartridgePreview {
    file_path: PathBuf,
    pub file_name: String,
    header: CartridgeHeader,
}

impl CartridgePreview {
    pub fn new(file_path: PathBuf) -> io::Result<Self> {
        let mut raw_header = [0u8; HEADER_SIZE];
        let file = File::open(&file_path)?;
        file.read_exact_at(&mut raw_header, 0)?;

        Ok(CartridgePreview {
            file_path: file_path.clone(),
            file_name: file_path.file_name().unwrap().to_str().unwrap().to_string(),
            header: unsafe { mem::transmute(raw_header) },
        })
    }

    pub fn read_icon(&self) -> io::Result<[u32; 32 * 32]> {
        let mut icon = [0u32; 32 * 32];

        let offset = self.header.icon_title_offset;
        if offset == 0 {
            return Err(io::Error::from(ErrorKind::InvalidData));
        }

        let file = File::open(&self.file_path)?;

        let mut data = [0u8; 0x200];
        file.read_exact_at(&mut data, offset as u64 + 0x20)?;

        let mut palette = [0u8; 0x20];
        file.read_exact_at(&mut palette, offset as u64 + 0x20 + data.len() as u64)?;

        let mut tiles = [0u32; 32 * 32];
        for i in 0..icon.len() {
            let pal_index = (data[i / 2] >> ((i & 1) * 4)) & 0xF;
            if pal_index == 0 {
                tiles[i] = 0xFFFFFFFF;
            } else {
                let color = utils::read_from_mem::<u16>(&palette, pal_index as u32 * 2);
                tiles[i] = rgb5_to_rgb8(color);
            }
        }

        for i in 0..4 {
            for j in 0..8 {
                for k in 0..4 {
                    let icon_start = 256 * i + 32 * j + 8 * k;
                    let tiles_start = 256 * i + 8 * j + 64 * k;
                    icon[icon_start..icon_start + 8].copy_from_slice(&tiles[tiles_start..tiles_start + 8])
                }
            }
        }

        Ok(icon)
    }

    pub fn read_title(&self) -> io::Result<String> {
        let offset = self.header.icon_title_offset;
        if offset == 0 {
            return Err(io::Error::from(ErrorKind::InvalidData));
        }

        let mut title = [0u8; 0x100];
        let file = File::open(&self.file_path)?;
        file.read_exact_at(&mut title, offset as u64 + 0x340)?;

        let (_, title, _) = unsafe { title.align_to() };
        let nul_pos = title.iter().position(|b| *b == 0);
        let end = match nul_pos {
            None => title.len(),
            Some(pos) => pos,
        };

        match String::from_utf16(&title[..end]) {
            Ok(title) => Ok(title),
            Err(_) => Err(io::Error::from(ErrorKind::InvalidData)),
        }
    }
}

pub struct CartridgeIo {
    file: File,
    pub file_name: String,
    pub file_size: u32,
    pub header: CartridgeHeader,
    content_pages: NoHashMap<u32, u16>,
    content_cache: HeapArrayU8<MAX_CARTRIDGE_CACHE>,
    save_file_path: PathBuf,
    pub save_file_size: u32,
    save_buf: Mutex<(Vec<u8>, bool)>,
    pub overlays: Vec<FsOverlayInfoHeader>,
}

unsafe impl Send for CartridgeIo {}

impl CartridgeIo {
    pub fn from_preview(preview: CartridgePreview, save_file_path: PathBuf) -> io::Result<Self> {
        let mut file = File::open(&preview.file_path)?;
        let file_size = file.stream_len().unwrap() as u32;
        let mut save_buf = Vec::new();

        let mut save_file_size = File::open(&save_file_path).map_or(0, |mut file| {
            let save_file_size = file.stream_len().unwrap();
            save_buf.resize(save_file_size as usize, 0u8);
            match file.read_at(&mut save_buf, 0) {
                Ok(_) => save_file_size as u32,
                Err(_) => {
                    save_buf.clear();
                    0
                }
            }
        });

        let game_code = u32::from_le_bytes(preview.header.game_code);
        if let Some(metadata) = get_cartridge_metadata(game_code) {
            save_buf.resize(metadata.save_size as usize, 0xFF);
            save_file_size = metadata.save_size;
        }

        if !SAVE_SIZES.contains(&save_file_size) {
            save_file_size = 0;
        }

        Ok(CartridgeIo {
            file,
            file_name: preview.file_name,
            file_size,
            header: preview.header,
            content_pages: NoHashMap::default(),
            content_cache: HeapArrayU8::default(),
            save_file_path,
            save_file_size,
            save_buf: Mutex::new((save_buf, false)),
            overlays: Vec::new(),
        })
    }

    fn get_page(&mut self, page_addr: u32) -> io::Result<*const [u8; CARTRIDGE_PAGE_SIZE]> {
        debug_assert_eq!(page_addr & (CARTRIDGE_PAGE_SIZE as u32 - 1), 0);
        match self.content_pages.get(&page_addr) {
            None => {
                if self.content_pages.len() >= MAX_CARTRIDGE_CACHE / CARTRIDGE_PAGE_SIZE {
                    debug_println!("clear cartridge pages");
                    self.content_pages.clear();
                }

                let content_offset = self.content_pages.len() as u16;
                let start = content_offset as usize * CARTRIDGE_PAGE_SIZE;
                let buf = &mut self.content_cache[start..start + CARTRIDGE_PAGE_SIZE];
                self.file.read_at(buf, page_addr as u64)?;
                self.content_pages.insert(page_addr, content_offset);
                Ok(buf.as_ptr() as _)
            }
            Some(page) => Ok(self.content_cache[*page as usize * CARTRIDGE_PAGE_SIZE..].as_ptr() as _),
        }
    }

    pub fn read_slice(&mut self, offset: u32, slice: &mut [u8]) -> io::Result<()> {
        let mut remaining = slice.len();
        while remaining > 0 {
            let slice_start = slice.len() - remaining;

            let page_addr = (offset + slice_start as u32) & !(CARTRIDGE_PAGE_SIZE as u32 - 1);
            let page_offset = offset + slice_start as u32 - page_addr;
            let page = self.get_page(page_addr)?;
            let page = unsafe { page.as_ref_unchecked() };
            let page_slice = &page[page_offset as usize..];

            let read_amount = min(remaining, page_slice.len());
            let slice_end = slice_start + read_amount;
            slice[slice_start..slice_end].copy_from_slice(&page_slice[..read_amount]);
            remaining -= read_amount;
        }
        Ok(())
    }

    pub fn read_arm9_code(&mut self) -> Vec<u8> {
        let mut boot_code = vec![0u8; self.header.arm9_values.size as usize];
        self.read_slice(self.header.arm9_values.rom_offset, &mut boot_code).unwrap();

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

    pub fn read_arm7_code(&mut self) -> Vec<u8> {
        let mut boot_code = vec![0u8; self.header.arm7_values.size as usize];
        self.read_slice(self.header.arm7_values.rom_offset, &mut boot_code).unwrap();
        boot_code
    }

    pub fn resize_save_file(&mut self, new_size: u32) {
        let mut lock = self.save_buf.lock().unwrap();
        let (save_buf, _) = lock.deref_mut();
        save_buf.resize(new_size as usize, 0xFF);
        self.save_file_size = new_size;
    }

    pub fn read_save_buf(&self, addr: u32) -> u8 {
        let lock = self.save_buf.lock().unwrap();
        let (save_buf, _) = lock.deref();
        save_buf[addr as usize]
    }

    pub fn write_save_buf(&self, addr: u32, value: u8) {
        let mut lock = self.save_buf.lock().unwrap();
        let (save_buf, dirty) = lock.deref_mut();
        save_buf[addr as usize] = value;
        *dirty = true;
    }

    pub fn write_save_buf_slice(&self, addr: u32, buf: &[u8]) {
        let mut lock = self.save_buf.lock().unwrap();
        let (save_buf, dirty) = lock.deref_mut();
        let write_len = min(buf.len(), save_buf.len());
        save_buf[addr as usize..addr as usize + write_len].copy_from_slice(&buf[..write_len]);
        *dirty = true;
    }

    pub fn flush_save_buf(&mut self, last_save_time: &Arc<Mutex<Option<(Instant, bool)>>>) {
        let mut lock = self.save_buf.lock().unwrap();
        let (save_buf, dirty) = lock.deref_mut();
        if *dirty {
            let success = File::create(&self.save_file_path).is_ok_and(|file| file.write_at(save_buf, 0).is_ok());
            *last_save_time.lock().unwrap() = Some((Instant::now(), success));
            *dirty = false;
        }
    }

    pub fn parse_overlays(&mut self) {
        const INFO_HEADER_SIZE: usize = size_of::<FsOverlayInfoHeader>();
        let mut id = 0;
        while (id * INFO_HEADER_SIZE as u32) < self.header.arm9_overlay.overlay_size {
            let offset = id * INFO_HEADER_SIZE as u32;
            let mut header = [0u8; INFO_HEADER_SIZE];
            if self.read_slice(self.header.arm9_overlay.overlay_offset + offset, &mut header).is_err() {
                break;
            }
            let header: FsOverlayInfoHeader = unsafe { mem::transmute(header) };
            self.overlays.push(header);
            id += 1;
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

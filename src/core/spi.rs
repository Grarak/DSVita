use crate::core::emu::Emu;
use crate::logging::debug_println;
use crate::presenter::{PRESENTER_AUDIO_IN_BUF_SIZE, PRESENTER_AUDIO_IN_SAMPLE_RATE};
use crate::settings::{Language, Settings};
use crate::utils;
use crate::utils::{HeapArray, HeapArrayI16, HeapArrayU8};
use bilge::prelude::*;
use std::mem;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

const FIRMWARE_SIZE: usize = 128 * 1024;
pub const USER_SETTINGS_1_ADDR: usize = 0x1FF00;

const fn get_firmware(language: Language) -> [u8; FIRMWARE_SIZE] {
    let mut firmware = [0u8; FIRMWARE_SIZE];

    // Set some firmware header data
    firmware[0x20] = 0xC0; // User settings offset / 8, byte 1
    firmware[0x21] = 0x3F; // User settings offset / 8, byte 2

    // Set some WiFi config data
    firmware[0x2C] = 0x38; // Config length, byte 1
    firmware[0x2D] = 0x01; // Config length, byte 2
    firmware[0x36] = 0x00; // MAC address, byte 1
    firmware[0x37] = 0x09; // MAC address, byte 2
    firmware[0x38] = 0xBF; // MAC address, byte 3
    firmware[0x39] = 0x12; // MAC address, byte 4
    firmware[0x3A] = 0x34; // MAC address, byte 5
    firmware[0x3B] = 0x0; // MAC address, byte 6
    firmware[0x3C] = 0xFE; // Enabled channels, byte 1
    firmware[0x3D] = 0x3F; // Enabled channels, byte 2

    // Calculate the WiFi config CRC
    let crc = utils::crc16(0, &firmware, 0x2C, 0x138);
    firmware[0x2A] = (crc & 0xFF) as u8;
    firmware[0x2B] = ((crc & 0xFF00) >> 8) as u8;

    // Configure the WiFi access points
    let mut addr = 0x1FA00;
    while addr <= 0x1FC00 {
        // Set some access point data
        firmware[addr + 0xE7] = 0xFF; // Not configured
        firmware[addr + 0xF5] = 0x28; // Unknown

        // Calculate the access point CRC
        let crc = utils::crc16(0, &firmware, addr, 0xFE);
        firmware[addr + 0xFE] = (crc & 0xFF) as u8;
        firmware[addr + 0xFF] = ((crc & 0xFF00) >> 8) as u8;

        addr += 0x100;
    }

    // Configure the user settings
    let mut addr = USER_SETTINGS_1_ADDR - 0x100;
    while addr <= USER_SETTINGS_1_ADDR {
        // Set some user settings data
        firmware[addr] = 5; // Version
        firmware[addr + 0x02] = 2; // Favorite color
        firmware[addr + 0x03] = 5; // Birthday month
        firmware[addr + 0x04] = 25; // Birthday day
        firmware[addr + 0x06] = b'D'; // Nickname, char 1
        firmware[addr + 0x08] = b'S'; // Nickname, char 2
        firmware[addr + 0x0A] = b'V'; // Nickname, char 3
        firmware[addr + 0x0C] = b'I'; // Nickname, char 4
        firmware[addr + 0x0E] = b'T'; // Nickname, char 5
        firmware[addr + 0x10] = b'A'; // Nickname, char 6
        firmware[addr + 0x1A] = 6; // Nickname length

        // Set the touch calibration data
        firmware[addr + 0x5E] = 0xF0; // ADC X2, byte 1
        firmware[addr + 0x5F] = 0x0F; // ADC X2, byte 2
        firmware[addr + 0x60] = 0xF0; // ADC Y2, byte 1
        firmware[addr + 0x61] = 0x0B; // ADC Y2, byte 2
        firmware[addr + 0x62] = 0xFF; // SCR X2
        firmware[addr + 0x63] = 0xBF; // SCR Y2

        // Set the language specified by the frontend
        firmware[addr + 0x64] = language as u8;

        // Calculate the user settings CRC
        let crc = utils::crc16(0xFFFF, &firmware, addr, 0x70);
        firmware[addr + 0x72] = (crc & 0xFF) as u8;
        firmware[addr + 0x73] = ((crc & 0xFF00) >> 8) as u8;

        addr += 0x100;
    }

    firmware
}

static DEFAULT_SPI_FIRMWARE: [u8; FIRMWARE_SIZE] = get_firmware(Language::English);

#[repr(u8)]
#[derive(Debug)]
enum SpiDevice {
    PowerManager = 0,
    Firmware = 1,
    Touchscreen = 2,
    Reserved = 3,
}

impl From<u8> for SpiDevice {
    fn from(value: u8) -> Self {
        debug_assert!(value <= SpiDevice::Reserved as u8);
        unsafe { mem::transmute(value) }
    }
}

pub const MIC_SAMPLE_CYCLES: u32 = 60 * 263 * 355 * 6 / PRESENTER_AUDIO_IN_SAMPLE_RATE as u32;

#[bitsize(16)]
#[derive(FromBits)]
struct SpiCnt {
    baudrate: u2,
    not_used: u5,
    busy_flag: bool,
    device_select: u2,
    transfer_size: u1,
    chip_select_hold: bool,
    not_used1: u2,
    interrupt_request: bool,
    spi_bus_enable: bool,
}

pub struct MicSampler {
    data: [HeapArrayI16<PRESENTER_AUDIO_IN_BUF_SIZE>; 2],
}

impl MicSampler {
    pub fn new() -> Self {
        MicSampler {
            data: [HeapArray::default(), HeapArray::default()],
        }
    }

    pub fn push(&mut self, buf: &mut HeapArrayI16<PRESENTER_AUDIO_IN_BUF_SIZE>) {
        mem::swap(buf, &mut self.data[1]);
    }

    pub fn consume(&mut self, buf: &mut HeapArrayI16<PRESENTER_AUDIO_IN_BUF_SIZE>) {
        self.data.swap(0, 1);
        self.data[1].fill(0);
        mem::swap(buf, &mut self.data[0]);
        self.data[0].fill(0);
    }
}

pub struct Spi {
    pub cnt: u16,
    pub data: u8,
    write_count: usize,
    cmd: u8,
    addr: u32,
    touch_points: Arc<AtomicU16>,
    pub firmware: HeapArrayU8<FIRMWARE_SIZE>,
    last_mic_sample: u16,
    pub mic_sample_cycle: u32,
    pub mic_samples: HeapArray<i16, { PRESENTER_AUDIO_IN_BUF_SIZE }>,
    pub mic_sampler: Arc<Mutex<MicSampler>>,
}

impl Spi {
    pub fn new(touch_points: Arc<AtomicU16>, mic_sampler: Arc<Mutex<MicSampler>>) -> Self {
        Spi {
            cnt: 0,
            data: 0,
            write_count: 0,
            cmd: 0,
            addr: 0,
            touch_points,
            firmware: HeapArrayU8::default(),
            last_mic_sample: 0,
            mic_sample_cycle: 0,
            mic_samples: HeapArray::default(),
            mic_sampler,
        }
    }

    pub fn init(&mut self, settings: &Settings) {
        self.cnt = 0;
        self.data = 0;
        self.write_count = 0;
        self.cmd = 0;
        self.addr = 0;
        self.firmware.copy_from_slice(&get_firmware(settings.language()));
        self.last_mic_sample = 0;
        self.mic_sample_cycle = 0;
    }

    pub fn set_cnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0xCF03;
        self.cnt = (self.cnt & !mask) | (value & mask);
    }

    pub fn get_touch_coordinates(&self) -> (u16, u16) {
        const ADC_X1: i32 = u16::from_le_bytes([DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0xA8], DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0xA7]]) as i32;
        const ADC_Y1: i32 = u16::from_le_bytes([DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0xA6], DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0xA5]]) as i32;
        const SCR_X1: i32 = DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0xA4] as i32;
        const SCR_Y1: i32 = DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0xA3] as i32;
        const ADC_X2: i32 = u16::from_le_bytes([DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0xA2], DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0xA1]]) as i32;
        const ADC_Y2: i32 = u16::from_le_bytes([DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0xA0], DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0x9F]]) as i32;
        const SCR_X2: i32 = DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0x9E] as i32;
        const SCR_Y2: i32 = DEFAULT_SPI_FIRMWARE[FIRMWARE_SIZE - 0x9D] as i32;

        let points = self.touch_points.load(Ordering::Relaxed);
        let x = points & 0xFF;
        let x = x.clamp(1, 254) as i32;
        let y = points >> 8;
        let y = y.clamp(1, 190) as i32;

        let touch_x = (x - SCR_X1 + 1) * (ADC_X2 - ADC_X1) / (SCR_X2 - SCR_X1) + ADC_X1;
        let touch_y = (y - SCR_Y1 + 1) * (ADC_Y2 - ADC_Y1) / (SCR_Y2 - SCR_Y1) + ADC_Y1;

        (touch_x as u16, touch_y as u16)
    }
}

impl Emu {
    pub fn spi_set_data(&mut self, value: u8) {
        let cnt = SpiCnt::from(self.spi.cnt);
        if !cnt.spi_bus_enable() {
            self.spi.data = 0;
            return;
        }

        if self.spi.write_count == 0 {
            self.spi.cmd = value;
            self.spi.addr = 0;
            self.spi.data = 0;
        } else {
            let device = SpiDevice::from(u8::from(cnt.device_select()));
            match device {
                SpiDevice::Firmware => {
                    if self.spi.cmd == 3 {
                        if self.spi.write_count < 4 {
                            self.spi.addr <<= 8;
                            self.spi.addr |= value as u32;
                        } else {
                            self.spi.data = if self.spi.addr < FIRMWARE_SIZE as u32 { self.spi.firmware[self.spi.addr as usize] } else { 0 };
                            self.spi.addr += u32::from(cnt.transfer_size()) + 1;
                        }
                    } else {
                        debug_println!("Unknown spi {:?} command {:x}", device, self.spi.cmd);
                        self.spi.data = 0;
                    }
                }
                SpiDevice::Touchscreen => {
                    self.spi.data = match (self.spi.cmd & 0x70) >> 4 {
                        1 => {
                            let y = self.spi.get_touch_coordinates().1;
                            if self.spi.write_count & 1 != 0 {
                                (y >> 5) as u8
                            } else {
                                (y << 3) as u8
                            }
                        }
                        5 => {
                            let x = self.spi.get_touch_coordinates().0;
                            if self.spi.write_count & 1 != 0 {
                                (x >> 5) as u8
                            } else {
                                (x << 3) as u8
                            }
                        }
                        6 => {
                            if self.spi.write_count & 1 != 0 {
                                let sample = (self.spi_mic_sample() >> 4) + 0x800;
                                self.spi.last_mic_sample = sample as u16;
                                (self.spi.last_mic_sample >> 5) as u8
                            } else {
                                (self.spi.last_mic_sample << 3) as u8
                            }
                        }
                        _ => 0,
                    }
                }
                _ => {
                    debug_println!("Unknown spi device {:?}", device);
                    self.spi.data = 0;
                }
            }
        }

        if cnt.chip_select_hold() {
            self.spi.write_count += 1;
        } else {
            self.spi.write_count = 0;
        }

        if cnt.interrupt_request() {
            todo!()
        }
    }

    pub fn spi_mic_sample(&mut self) -> i16 {
        let cycle_diff = self.cm.get_cycles() - self.spi.mic_sample_cycle;
        let mut index = (cycle_diff / MIC_SAMPLE_CYCLES) as usize;
        if index >= self.spi.mic_samples.len() {
            {
                let mut mic_sampler = self.spi.mic_sampler.lock().unwrap();
                mic_sampler.consume(&mut self.spi.mic_samples);
            }
            self.spi.mic_sample_cycle = self.cm.get_cycles();
            index = 0;
        }
        self.spi.mic_samples[index]
    }
}

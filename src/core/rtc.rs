use crate::logging::debug_println;
use crate::IS_DEBUG;
use bilge::prelude::*;
use chrono::{Datelike, Timelike};

#[bitsize(8)]
#[derive(FromBits)]
struct RtcReg {
    data_io: u1,
    clock_out: u1,
    select_out: u1,
    not_used: u1,
    data_dir: u1,
    clock_dir: u1,
    select_dir: u1,
    not_used1: u1,
}

#[derive(Default)]
pub struct Rtc {
    rtc: u8,
    cs: bool,
    sck: bool,
    sio: bool,
    write_count: u8,
    cmd: u8,
    cnt: u8,
    date_time: [u8; 7],
}

impl Rtc {
    pub fn new() -> Self {
        Rtc::default()
    }

    pub fn get_rtc(&self) -> u8 {
        let mut reg = RtcReg::from(self.rtc);

        let cs = self.cs;
        let sck = !bool::from(reg.clock_dir()) && self.sck;
        let sio = !bool::from(reg.data_dir()) && self.sio;

        reg.set_select_out(u1::new(cs as u8));
        reg.set_clock_out(u1::new(sck as u8));
        reg.set_data_io(u1::new(sio as u8));

        u8::from(reg)
    }

    pub fn set_rtc(&mut self, value: u8) {
        self.rtc = value & !0x7;

        let dir_reg = RtcReg::from(self.rtc);
        let data_reg = RtcReg::from(value);

        let cs = if bool::from(dir_reg.select_dir()) { bool::from(data_reg.select_out()) } else { self.cs };
        let sck = if bool::from(dir_reg.clock_dir()) { !bool::from(data_reg.clock_out()) } else { self.sck };
        let sio = if bool::from(dir_reg.data_dir()) { bool::from(data_reg.data_io()) } else { self.sio };

        self.update_rtc(cs, sck, sio);
    }

    fn update_rtc(&mut self, cs: bool, sck: bool, mut sio: bool) {
        if cs {
            if !self.sck && sck {
                if self.write_count < 8 {
                    self.cmd |= (sio as u8) << (7 - self.write_count);

                    if self.write_count == 7 && (self.cmd & 0xF0) != 0x60 {
                        self.cmd = self.cmd.reverse_bits();
                    }
                } else if self.cmd & 1 == 1 {
                    sio = self.read_reg((self.cmd >> 1) & 0x7);
                } else {
                    self.write_reg((self.cmd >> 1) & 0x7, sio);
                }
                self.write_count += 1;
            }
        } else {
            self.write_count = 0;
            self.cmd = 0;
        }

        self.cs = cs;
        self.sck = sck;
        self.sio = sio;
    }

    fn reset(&mut self) {
        self.update_rtc(false, false, false);
        self.cnt = 0;
        self.rtc = 0;
    }

    fn read_reg(&mut self, index: u8) -> bool {
        match index {
            0 => {
                self.reset();
                false
            }
            1 => (self.cnt >> (self.write_count & 7)) & 1 == 1,
            2 => {
                if self.write_count == 8 {
                    self.update_date_time();
                }
                (self.date_time[(self.write_count / 8 - 1) as usize] >> (self.write_count & 7)) & 1 == 1
            }
            3 => {
                if self.write_count == 8 {
                    self.update_date_time();
                }
                (self.date_time[(self.write_count / 8 + 3) as usize] >> (self.write_count & 7)) & 1 == 1
            }
            _ => {
                debug_println!("Read from unknown rtc register: {}", index);
                false
            }
        }
    }

    fn write_reg(&mut self, index: u8, value: bool) {
        match index {
            0 => {
                if (self.write_count & 7 == 0) && value {
                    self.reset();
                } else if ((1 << (self.write_count & 7)) & 0xE) != 0 {
                    self.cnt = (self.cnt & !(1 << (self.write_count & 7))) | ((value as u8) << (self.write_count & 7));
                }
            }
            _ => {
                debug_println!("Write to unknown rtc register: {}", index);
            }
        }
    }

    fn update_date_time(&mut self) {
        let (year, month, day, hour, is_pm, min, sec) = if IS_DEBUG {
            (2000, 1, 1, 11, false, 0, 0)
        } else {
            let local_now = chrono::Local::now();

            let year = local_now.year() as u32 % 100;
            let month = local_now.month() as u8;
            let day = local_now.day() as u8;
            let (hour, is_pm) = {
                let hour = local_now.hour();
                ((if self.cnt & 0x2 == 0 { hour % 12 } else { hour }) as u8, hour >= 12)
            };
            let min = local_now.minute() as u8;
            let sec = local_now.second() as u8;

            (year, month, day, hour, is_pm, min, sec)
        };

        self.date_time[0] = (((year / 10) << 4) | (year % 10)) as u8;
        self.date_time[1] = ((month / 10) << 4) | (month % 10);
        self.date_time[2] = ((day / 10) << 4) | (day % 10);
        self.date_time[4] = ((hour / 10) << 4) | (hour % 10);
        self.date_time[4] |= (is_pm as u8) << 6;
        self.date_time[5] = ((min / 10) << 4) | (min % 10);
        self.date_time[6] = ((sec / 10) << 4) | (sec % 10);
    }
}

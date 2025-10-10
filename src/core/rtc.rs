use crate::IS_DEBUG;
use crate::{core::emu::Emu, logging::debug_println};
use bilge::prelude::*;
use chrono::{Datelike, Timelike};

#[bitsize(8)]
#[derive(Clone, Copy, FromBits)]
pub struct RtcReg {
    data_io: bool,
    clock_out: bool,
    select_out: bool,
    not_used: u1,
    data_dir_write: bool,
    clock_dir_write: bool,
    select_dir_write: bool,
    not_used1: u1,
}

impl Default for RtcReg {
    fn default() -> Self {
        RtcReg::from(0)
    }
}

#[derive(Default)]
pub struct Rtc {
    rtc: u8,
    select_out: bool,
    clock_out: bool,
    data_io: bool,
    write_count: u8,
    cmd: u8,
    cnt: u8,
    pub date_time: [u8; 7],
}

impl Emu {
    pub fn rtc_get(&mut self) {
        let mut reg = RtcReg::from(self.rtc.rtc);

        let cs = self.rtc.select_out;
        let sck = !reg.clock_dir_write() && self.rtc.clock_out;
        let sio = !reg.data_dir_write() && self.rtc.data_io;

        reg.set_select_out(cs);
        reg.set_clock_out(sck);
        reg.set_data_io(sio);

        self.mem.io.arm7().rtc = reg;
    }

    pub fn rtc_set(&mut self) {
        let data_reg = self.mem.io.arm7().rtc;
        self.rtc.rtc = data_reg.value & !0x7;

        let dir_reg = RtcReg::from(self.rtc.rtc);

        let cs = if dir_reg.select_dir_write() { data_reg.select_out() } else { self.rtc.select_out };
        let sck = if dir_reg.clock_dir_write() { !data_reg.clock_out() } else { self.rtc.clock_out };
        let sio = if dir_reg.data_dir_write() { data_reg.data_io() } else { self.rtc.data_io };

        self.rtc.update_rtc(cs, sck, sio);
    }
}

impl Rtc {
    pub fn new() -> Self {
        Rtc::default()
    }

    fn update_rtc(&mut self, select_out: bool, clock_out: bool, mut data_io: bool) {
        if select_out {
            if !self.clock_out && clock_out {
                if self.write_count < 8 {
                    self.cmd |= (data_io as u8) << (7 - self.write_count);

                    if self.write_count == 7 && (self.cmd & 0xF0) != 0x60 {
                        self.cmd = self.cmd.reverse_bits();
                    }
                } else if self.cmd & 1 == 1 {
                    data_io = self.read_reg((self.cmd >> 1) & 0x7);
                } else {
                    self.write_reg((self.cmd >> 1) & 0x7, data_io);
                }
                self.write_count += 1;
            }
        } else {
            self.write_count = 0;
            self.cmd = 0;
        }

        self.select_out = select_out;
        self.clock_out = clock_out;
        self.data_io = data_io;
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

    pub fn update_date_time(&mut self) {
        let (year, month, day, weekday, hour, is_pm, min, sec) = if IS_DEBUG {
            (2000, 1, 1, 0, 11, false, 0, 0)
        } else {
            let local_now = chrono::Local::now();

            let year = local_now.year() as u32 % 100;
            let month = local_now.month() as u8;
            let day = local_now.day() as u8;
            let weekday = local_now.weekday() as u8;
            let (hour, is_pm) = {
                let hour = local_now.hour();
                ((if self.cnt & 0x2 == 0 { hour % 12 } else { hour }) as u8, hour >= 12)
            };
            let min = local_now.minute() as u8;
            let sec = local_now.second() as u8;

            (year, month, day, weekday, hour, is_pm, min, sec)
        };

        self.date_time[0] = (((year / 10) << 4) | (year % 10)) as u8;
        self.date_time[1] = ((month / 10) << 4) | (month % 10);
        self.date_time[2] = ((day / 10) << 4) | (day % 10);
        self.date_time[3] = ((weekday / 10) << 4) | (weekday % 10);
        self.date_time[4] = ((hour / 10) << 4) | (hour % 10);
        self.date_time[4] |= (is_pm as u8) << 6;
        self.date_time[5] = ((min / 10) << 4) | (min % 10);
        self.date_time[6] = ((sec / 10) << 4) | (sec % 10);
    }
}

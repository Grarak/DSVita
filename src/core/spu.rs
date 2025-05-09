use crate::core::cycle_manager::EventType;
use crate::core::emu::Emu;
use crate::core::CpuType::ARM7;
use crate::presenter::{PRESENTER_AUDIO_BUF_SIZE, PRESENTER_AUDIO_SAMPLE_RATE};
use crate::utils::HeapMemU32;
use bilge::prelude::*;
use std::cmp::min;
use std::collections::VecDeque;
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::intrinsics::{likely, unlikely};
use std::mem;
use std::sync::{Arc, Condvar, Mutex};

pub const CHANNEL_COUNT: usize = 16;
const SAMPLE_RATE: usize = 32768;
const SAMPLE_BUFFER_SIZE: usize = SAMPLE_RATE * PRESENTER_AUDIO_BUF_SIZE / PRESENTER_AUDIO_SAMPLE_RATE;

pub struct SoundSampler {
    samples: Mutex<VecDeque<HeapMemU32<{ SAMPLE_BUFFER_SIZE }>>>,
    cond_var: Condvar,
    framelimit: bool,
}

impl SoundSampler {
    pub fn new(framelimit: bool) -> SoundSampler {
        SoundSampler {
            samples: Mutex::new(VecDeque::new()),
            cond_var: Condvar::new(),
            framelimit,
        }
    }

    fn push(&self, samples: &[u32]) {
        {
            let mut queue = self.samples.lock().unwrap();
            let mut queue = if queue.len() == 4 {
                if self.framelimit {
                    self.cond_var.wait_while(queue, |queue| queue.len() == 4).unwrap()
                } else {
                    queue.pop_front();
                    queue
                }
            } else {
                queue
            };
            let mut s = HeapMemU32::new();
            s.copy_from_slice(samples);
            queue.push_back(s);
        }
        self.cond_var.notify_one();
    }

    pub fn consume(&self, ret: &mut [u32; PRESENTER_AUDIO_BUF_SIZE]) {
        let samples = {
            let samples = self.samples.lock().unwrap();
            let mut samples = self.cond_var.wait_while(samples, |samples| samples.is_empty()).unwrap();
            samples.pop_front().unwrap()
        };
        self.cond_var.notify_one();
        for i in 0..PRESENTER_AUDIO_BUF_SIZE {
            ret[i] = samples[i * SAMPLE_BUFFER_SIZE / PRESENTER_AUDIO_BUF_SIZE];
        }
    }
}

const ADPCM_TABLE: [i16; 89] = [
    0x0007, 0x0008, 0x0009, 0x000A, 0x000B, 0x000C, 0x000D, 0x000E, 0x0010, 0x0011, 0x0013, 0x0015, 0x0017, 0x0019, 0x001C, 0x001F, 0x0022, 0x0025, 0x0029, 0x002D, 0x0032, 0x0037, 0x003C, 0x0042,
    0x0049, 0x0050, 0x0058, 0x0061, 0x006B, 0x0076, 0x0082, 0x008F, 0x009D, 0x00AD, 0x00BE, 0x00D1, 0x00E6, 0x00FD, 0x0117, 0x0133, 0x0151, 0x0173, 0x0198, 0x01C1, 0x01EE, 0x0220, 0x0256, 0x0292,
    0x02D4, 0x031C, 0x036C, 0x03C3, 0x0424, 0x048E, 0x0502, 0x0583, 0x0610, 0x06AB, 0x0756, 0x0812, 0x08E0, 0x09C3, 0x0ABD, 0x0BD0, 0x0CFF, 0x0E4C, 0x0FBA, 0x114C, 0x1307, 0x14EE, 0x1706, 0x1954,
    0x1BDC, 0x1EA5, 0x21B6, 0x2515, 0x28CA, 0x2CDF, 0x315B, 0x364B, 0x3BB9, 0x41B2, 0x4844, 0x4F7E, 0x5771, 0x602F, 0x69CE, 0x7462, 0x7FFF,
];

const fn calculate_adpcm_diff_table() -> [[i32; 16]; 89] {
    let mut table = [[0; 16]; 89];
    let mut i = 0;
    while i < 16 {
        let mut j = 0;
        while j < 89 {
            table[j][i] = ADPCM_TABLE[j] as i32 / 8;
            if i & 1 != 0 {
                table[j][i] += ADPCM_TABLE[j] as i32 / 4;
            }
            if i & 2 != 0 {
                table[j][i] += ADPCM_TABLE[j] as i32 / 2;
            }
            if i & 4 != 0 {
                table[j][i] += ADPCM_TABLE[j] as i32;
            }
            if i & 8 == 0 {
                table[j][i] = -table[j][i]
            }
            j += 1;
        }
        i += 1;
    }
    table
}
const ADPCM_DIFF_TABLE: [[i32; 16]; 89] = calculate_adpcm_diff_table();

const fn calculate_adpcm_index_table() -> [[u8; 8]; 89] {
    let mut table = [[0; 8]; 89];
    let mut i = 0;
    while i < 8 {
        let mut j = 0;
        while j < 89 {
            const INDICES: [i8; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];
            let mut index = j as i8 + INDICES[i];
            if index < 0 {
                index = 0;
            } else if index > 88 {
                index = 88;
            }
            table[j][i] = index as u8;
            j += 1;
        }
        i += 1;
    }
    table
}
const ADPCM_INDEX_TABLE: [[u8; 8]; 89] = calculate_adpcm_index_table();
// const ADPCM_INDEX_TABLE: [i8; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
pub struct SoundCnt {
    pub volume_mul: u7,
    not_used: u1,
    pub volume_div: u2,
    not_used1: u5,
    pub hold: bool,
    pub panning: u7,
    not_used2: u1,
    pub wave_duty: u3,
    pub repeat_mode: u2,
    pub format: u2,
    pub start_status: bool,
}

impl SoundCnt {
    fn get_format(&self) -> SoundChannelFormat {
        SoundChannelFormat::from(u8::from(self.format()))
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SoundChannelFormat {
    Pcm8 = 0,
    Pcm16 = 1,
    ImaAdpcm = 2,
    PsgNoise = 3,
}

impl Default for SoundChannelFormat {
    fn default() -> Self {
        SoundChannelFormat::Pcm8
    }
}

impl From<u8> for SoundChannelFormat {
    fn from(value: u8) -> Self {
        debug_assert!(value <= SoundChannelFormat::PsgNoise as u8);
        unsafe { mem::transmute(value) }
    }
}

#[bitsize(16)]
#[derive(Copy, Clone, Default, FromBits)]
pub struct MainSoundCnt {
    pub master_volume: u7,
    not_used: u1,
    pub left_output_from: u2,
    pub right_output_from: u2,
    pub output_ch_to_mixer: u2,
    not_used2: u1,
    pub master_enable: bool,
}

#[derive(Copy, Clone, Default)]
struct SpuChannel {
    cnt: SoundCnt,
    sad: u32,
    tmr: u16,
    pnt: u16,
    len: u32,
    sad_current: u32,
    tmr_current: u32,
    adpcm_value: i16,
    adpcm_loop_value: i16,
    adpcm_index: u8,
    adpcm_loop_index: u8,
    adpcm_toggle: bool,
    active: bool,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
struct AdpcmHeader {
    pcm16_value: u16,
    table_index: u7,
    not_used: u9,
}

#[bitsize(8)]
#[derive(Copy, Clone, Default, FromBits)]
pub struct SoundCapCnt {
    pub cnt_associated_channels: bool,
    pub cap_src_select: bool,
    pub one_shot: bool,
    pub pcm8: bool,
    not_used: u3,
    pub start_status: bool,
}

#[derive(Copy, Clone, Default)]
struct SoundCapChannel {
    cnt: SoundCapCnt,
    dad: u32,
    len: u16,
    dad_current: u32,
    tmr_current: u32,
}

pub struct Spu {
    pub audio_enabled: bool,
    channels: [SpuChannel; CHANNEL_COUNT],
    sound_cap_channels: [SoundCapChannel; 2],
    main_sound_cnt: MainSoundCnt,
    sound_bias: u16,
    duty_cycles: [i32; 6],
    noise_values: [u16; 2],
    samples_buffer: Vec<u32>,
    sound_sampler: Arc<SoundSampler>,
}

impl Spu {
    pub fn new(sound_sampler: Arc<SoundSampler>) -> Self {
        Spu {
            audio_enabled: false,
            channels: [SpuChannel::default(); CHANNEL_COUNT],
            sound_cap_channels: [SoundCapChannel::default(); 2],
            main_sound_cnt: MainSoundCnt::from(0),
            sound_bias: 0,
            duty_cycles: [0; 6],
            noise_values: [0; 2],
            samples_buffer: Vec::with_capacity(SAMPLE_BUFFER_SIZE),
            sound_sampler,
        }
    }
}

impl Emu {
    pub fn spu_initialize_schedule(&mut self) {
        self.cm.schedule(512 * 2, EventType::SpuSample, 0);
    }

    pub fn spu_get_cnt(&self, channel_num: usize) -> u32 {
        self.spu.channels[channel_num].cnt.into()
    }

    pub fn spu_get_main_sound_cnt(&self) -> u16 {
        self.spu.main_sound_cnt.into()
    }

    pub fn spu_get_snd_cap_cnt(&self, channel_num: usize) -> u8 {
        self.spu.sound_cap_channels[channel_num].cnt.into()
    }

    pub fn spu_set_cnt(&mut self, channel_num: usize, mut mask: u32, value: u32) {
        let channel = &mut self.spu.channels[channel_num];
        let was_disabled = !channel.cnt.start_status();

        mask &= 0xFF7F837F;
        channel.cnt = ((u32::from(channel.cnt) & !mask) | (value & mask)).into();

        if was_disabled && channel.cnt.start_status() && self.spu.main_sound_cnt.master_enable() && (channel.sad != 0 || channel.cnt.get_format() == SoundChannelFormat::PsgNoise) {
            self.spu_start_channel(channel_num);
        } else if !channel.cnt.start_status() {
            channel.active = false;
        }
    }

    pub fn spu_set_sad(&mut self, channel_num: usize, mut mask: u32, value: u32) {
        let channel = &mut self.spu.channels[channel_num];
        mask &= 0x07FFFFFC;
        channel.sad = (channel.sad & !mask) | (value & mask);

        if channel.cnt.get_format() != SoundChannelFormat::PsgNoise {
            if channel.sad != 0 && (self.spu.main_sound_cnt.master_enable() && channel.cnt.start_status()) {
                self.spu_start_channel(channel_num);
            } else {
                channel.active = false;
            }
        }
    }

    pub fn spu_set_tmr(&mut self, channel_num: usize, mask: u16, value: u16) {
        self.spu.channels[channel_num].tmr = (self.spu.channels[channel_num].tmr & !mask) | (value & mask);
    }

    pub fn spu_set_pnt(&mut self, channel_num: usize, mask: u16, value: u16) {
        self.spu.channels[channel_num].pnt = (self.spu.channels[channel_num].pnt & !mask) | (value & mask);
    }

    pub fn spu_set_len(&mut self, channel_num: usize, mut mask: u32, value: u32) {
        mask &= 0x003FFFFF;
        self.spu.channels[channel_num].len = (self.spu.channels[channel_num].len & !mask) | (value & mask);
    }

    pub fn spu_set_main_sound_cnt(&mut self, mut mask: u16, value: u16) {
        let was_disabled = !self.spu.main_sound_cnt.master_enable();

        mask &= 0xBF7F;
        self.spu.main_sound_cnt = ((u16::from(self.spu.main_sound_cnt) & !mask) | (value & mask)).into();

        if was_disabled && self.spu.main_sound_cnt.master_enable() {
            for i in 0..CHANNEL_COUNT {
                if self.spu.channels[i].cnt.start_status() && (self.spu.channels[i].sad != 0 || self.spu.channels[i].cnt.get_format() == SoundChannelFormat::PsgNoise) {
                    self.spu_start_channel(i);
                }
            }
        } else if !self.spu.main_sound_cnt.master_enable() {
            for channel in &mut self.spu.channels {
                channel.active = false;
            }
        }
    }

    pub fn spu_set_sound_bias(&mut self, mut mask: u16, value: u16) {
        mask &= 0x03FF;
        self.spu.sound_bias = (self.spu.sound_bias & !mask) | (value & mask);
    }

    pub fn spu_set_snd_cap_cnt(&mut self, channel_num: usize, value: u8) {
        let cap_channel = &mut self.spu.sound_cap_channels[channel_num];
        let cnt = SoundCapCnt::from(value & 0x8F);
        if !cap_channel.cnt.start_status() && cnt.start_status() {
            cap_channel.dad_current = cap_channel.dad;
            cap_channel.tmr_current = self.spu.channels[(channel_num << 1) + 1].tmr as u32;
        }

        cap_channel.cnt = cnt;
    }

    pub fn spu_set_snd_cap_dad(&mut self, channel_num: usize, mut mask: u32, value: u32) {
        mask &= 0x07FFFFFC;
        let cap_channel = &mut self.spu.sound_cap_channels[channel_num];
        cap_channel.dad = (cap_channel.dad & !mask) | (value & mask);

        cap_channel.dad_current = cap_channel.dad;
        cap_channel.tmr_current = self.spu.channels[(channel_num << 1) + 1].tmr as u32;
    }

    pub fn spu_set_snd_cap_len(&mut self, channel: usize, mask: u16, value: u16) {
        let cap_channel = &mut self.spu.sound_cap_channels[channel];
        cap_channel.len = (cap_channel.len & !mask) | (value & mask);
    }

    fn spu_start_channel(&mut self, channel_num: usize) {
        self.spu.channels[channel_num].sad_current = self.spu.channels[channel_num].sad;
        self.spu.channels[channel_num].tmr_current = self.spu.channels[channel_num].tmr as u32;

        match self.spu.channels[channel_num].cnt.get_format() {
            SoundChannelFormat::ImaAdpcm => {
                let header = AdpcmHeader::from(self.mem_read::<{ ARM7 }, u32>(self.spu.channels[channel_num].sad_current));
                self.spu.channels[channel_num].adpcm_value = header.pcm16_value() as i16;
                self.spu.channels[channel_num].adpcm_index = min(u8::from(header.table_index()), 88);
                self.spu.channels[channel_num].adpcm_toggle = false;
                self.spu.channels[channel_num].sad_current += 4;
            }
            SoundChannelFormat::PsgNoise => {
                if (8..=13).contains(&channel_num) {
                    self.spu.duty_cycles[channel_num - 8] = 0;
                } else if channel_num >= 14 {
                    self.spu.noise_values[channel_num - 14] = 0x7FFF;
                }
            }
            _ => {}
        }

        self.spu.channels[channel_num].active = true;
    }

    fn spu_next_sample_pcm(&mut self, channel_num: usize) {
        self.spu.channels[channel_num].sad_current += 1 + u8::from(self.spu.channels[channel_num].cnt.format()) as u32;
    }

    fn spu_next_sample_psg(&mut self, channel_num: usize) {
        if (8..=13).contains(&channel_num) {
            self.spu.duty_cycles[channel_num - 8] = (self.spu.duty_cycles[channel_num - 8] + 1) % 8;
        } else if channel_num >= 14 {
            let noise_value = unsafe { self.spu.noise_values.get_unchecked_mut(channel_num - 14) };
            *noise_value &= !(1 << 15);
            if *noise_value & 1 != 0 {
                *noise_value = (1 << 15) | ((*noise_value >> 1) ^ 0x6000);
            } else {
                *noise_value >>= 1;
            }
        }
    }

    fn spu_next_sample_adpcm(&mut self, channel_num: usize) {
        let channel = &mut self.spu.channels[channel_num];
        unsafe { assert_unchecked(channel.adpcm_index < 89) };

        if channel.sad_current == channel.sad + ((channel.pnt as u32) << 2) && !channel.adpcm_toggle {
            channel.adpcm_loop_value = channel.adpcm_value;
            channel.adpcm_loop_index = channel.adpcm_index;
        }

        let sad_current = channel.sad_current;
        let adpcm_data = self.mem_read::<{ ARM7 }, u8>(sad_current);

        let channel = &mut self.spu.channels[channel_num];
        let adpcm_data = if channel.adpcm_toggle { adpcm_data >> 4 } else { adpcm_data & 0xF };

        let diff = ADPCM_DIFF_TABLE[channel.adpcm_index as usize][adpcm_data as usize];
        channel.adpcm_value = (channel.adpcm_value as i32 + diff).clamp(-0x8000, 0x7FFF) as i16;

        channel.adpcm_index = ADPCM_INDEX_TABLE[channel.adpcm_index as usize][(adpcm_data & 0x7) as usize];

        channel.adpcm_toggle = !channel.adpcm_toggle;
        if !channel.adpcm_toggle {
            channel.sad_current += 1;
        }
    }

    pub fn spu_on_sample_event(&mut self, _: u16) {
        if unlikely(!self.spu.audio_enabled) {
            for i in 0..CHANNEL_COUNT {
                self.spu.channels[i].cnt.set_start_status(false);
            }
            for i in 0..2 {
                self.spu.sound_cap_channels[i].cnt.set_start_status(false);
            }
            self.spu.samples_buffer.push(0);
            if unlikely(self.spu.samples_buffer.len() == SAMPLE_BUFFER_SIZE) {
                self.spu.sound_sampler.push(self.spu.samples_buffer.as_slice());
                self.spu.samples_buffer.clear();
            }
            self.cm.schedule(512 * 2, EventType::SpuSample, 0);
            return;
        }

        let mut mixer_left = 0;
        let mut mixer_right = 0;
        let mut channels_left = [0; 2];
        let mut channels_right = [0; 2];

        for i in 0..CHANNEL_COUNT {
            if likely(!self.spu.channels[i].active) {
                continue;
            }

            let channel = &mut self.spu.channels[i];
            let sad_current = channel.sad_current;
            let format = channel.cnt.get_format();

            let mut data = match format {
                SoundChannelFormat::Pcm8 => (self.mem_read::<{ ARM7 }, u8>(sad_current) as i8 as i32) << 8,
                SoundChannelFormat::Pcm16 => self.mem_read::<{ ARM7 }, u16>(sad_current) as i16 as i32,
                SoundChannelFormat::ImaAdpcm => channel.adpcm_value as i32,
                SoundChannelFormat::PsgNoise => {
                    if i >= 8 && i <= 13 {
                        let duty = 7 - u8::from(channel.cnt.wave_duty());
                        if self.spu.duty_cycles[i - 8] < duty as i32 {
                            -0x7FFF
                        } else {
                            0x7FFF
                        }
                    } else if i >= 14 {
                        if (self.spu.noise_values[i - 14] & (1 << 15)) != 0 {
                            -0x7FFF
                        } else {
                            0x7FFF
                        }
                    } else {
                        0
                    }
                }
            };

            let channel = &mut self.spu.channels[i];
            let mut tmr_current = channel.tmr_current + 512;
            let tmr = channel.tmr;
            while tmr_current >> 16 != 0 {
                tmr_current = tmr as u32 + (tmr_current - 0x10000);

                match format {
                    SoundChannelFormat::Pcm8 | SoundChannelFormat::Pcm16 => self.spu_next_sample_pcm(i),
                    SoundChannelFormat::ImaAdpcm => self.spu_next_sample_adpcm(i),
                    SoundChannelFormat::PsgNoise => self.spu_next_sample_psg(i),
                }

                let channel = &mut self.spu.channels[i];
                if format != SoundChannelFormat::PsgNoise && channel.sad_current >= (channel.sad + ((channel.pnt as u32 + channel.len) << 2)) {
                    if u8::from(channel.cnt.repeat_mode()) == 1 {
                        channel.sad_current = channel.sad + ((channel.pnt as u32) << 2);

                        if format == SoundChannelFormat::ImaAdpcm {
                            channel.adpcm_value = channel.adpcm_loop_value;
                            channel.adpcm_index = channel.adpcm_loop_index;
                            channel.adpcm_toggle = false;
                        }
                    } else {
                        channel.cnt.set_start_status(false);
                        channel.active = false;
                        break;
                    }
                }
            }
            let channel = &mut self.spu.channels[i];
            channel.tmr_current = tmr_current;

            let mut volume_mul = u8::from(channel.cnt.volume_mul());
            if volume_mul == 127 {
                volume_mul += 1;
            }
            data = (data * volume_mul as i32) >> 7;

            let mut volume_div = u8::from(channel.cnt.volume_div());
            if volume_div == 3 {
                volume_div += 1;
            }
            data >>= volume_div;

            let mut panning = u8::from(channel.cnt.panning());
            if panning == 127 {
                panning += 1;
            }
            let data_left = (data * (128 - panning as i32)) >> 7;
            let data_right = (data * panning as i32) >> 7;

            if i == 1 || i == 3 {
                let index = i >> 1;
                channels_left[index] = data_left;
                channels_right[index] = data_right;
                if u8::from(self.spu.main_sound_cnt.output_ch_to_mixer()) & (1 << index) != 0 {
                    continue;
                }
            }

            mixer_left += data_left;
            mixer_right += data_right;
        }

        for i in 0..2 {
            if !self.spu.sound_cap_channels[i].cnt.start_status() {
                continue;
            }

            let sample = if i == 0 { mixer_left } else { mixer_right };
            let sample = sample.clamp(-0x8000, 0x7FFF);

            let mut tmr_current = self.spu.sound_cap_channels[i].tmr_current + 512;
            let tmr = self.spu.channels[(i << 1) + 1].tmr;
            while tmr_current >> 16 != 0 {
                tmr_current = tmr as u32 + (tmr_current - 0x10000);

                if self.spu.sound_cap_channels[i].cnt.pcm8() {
                    self.mem_write::<{ ARM7 }, u8>(self.spu.sound_cap_channels[i].dad_current, (sample >> 8) as u8);
                    self.spu.sound_cap_channels[i].dad_current += 1;
                } else {
                    self.mem_write::<{ ARM7 }, u16>(self.spu.sound_cap_channels[i].dad_current, sample as u16);
                    self.spu.sound_cap_channels[i].dad_current += 2;
                }

                let channel = &mut self.spu.sound_cap_channels[i];
                if channel.dad_current >= (channel.dad + ((channel.len as u32) << 2)) {
                    if channel.cnt.one_shot() {
                        channel.cnt.set_start_status(false);
                    } else {
                        channel.dad_current = channel.dad;
                    }
                }
            }
            self.spu.sound_cap_channels[i].tmr_current = tmr_current;
        }

        let sample_left = match u8::from(self.spu.main_sound_cnt.left_output_from()) {
            0 => mixer_left,
            1 => channels_left[0],
            2 => channels_left[1],
            3 => channels_left[0] + channels_left[1],
            _ => unsafe { unreachable_unchecked() },
        };

        let sample_right = match u8::from(self.spu.main_sound_cnt.right_output_from()) {
            0 => mixer_right,
            1 => channels_right[0],
            2 => channels_right[1],
            3 => channels_right[0] + channels_right[1],
            _ => unsafe { unreachable_unchecked() },
        };

        let mut master_volume = u8::from(self.spu.main_sound_cnt.master_volume());
        if master_volume == 127 {
            master_volume += 1;
        }
        let sample_left = (sample_left * master_volume as i32) >> 7;
        let sample_right = (sample_right * master_volume as i32) >> 7;

        let sample_left = (sample_left >> 6) + self.spu.sound_bias as i32;
        let sample_right = (sample_right >> 6) + self.spu.sound_bias as i32;

        let sample_left = sample_left.clamp(0, 0x3FF);
        let sample_right = sample_right.clamp(0, 0x3FF);

        let sample_left = ((sample_left - 0x200) << 5) as u32;
        let sample_right = ((sample_right - 0x200) << 5) as u32;

        unsafe {
            assert_unchecked(self.spu.samples_buffer.len() < self.spu.samples_buffer.capacity());
            self.spu.samples_buffer.push_within_capacity((sample_right << 16) | (sample_left & 0xFFFF)).unwrap_unchecked();
        }
        if unlikely(self.spu.samples_buffer.len() == SAMPLE_BUFFER_SIZE) {
            self.spu.sound_sampler.push(self.spu.samples_buffer.as_slice());
            self.spu.samples_buffer.clear();
        }

        self.cm.schedule(512 * 2, EventType::SpuSample, 0);
    }
}

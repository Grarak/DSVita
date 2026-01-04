use crate::core::cycle_manager::EventType;
use crate::core::emu::Emu;
use crate::core::CpuType::ARM7;
use crate::logging::debug_println;
use crate::presenter::{PRESENTER_AUDIO_OUT_BUF_SIZE, PRESENTER_AUDIO_OUT_SAMPLE_RATE};
use crate::soundtouch::SoundTouch;
use crate::utils::HeapArrayU32;
use bilge::prelude::*;
use std::cmp::min;
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::intrinsics::unlikely;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread::Thread;
use std::time::Duration;
use std::{mem, slice, thread};

pub const CHANNEL_COUNT: usize = 16;
const SAMPLE_RATE: usize = 32768;
pub const SAMPLE_BUFFER_SIZE: usize = SAMPLE_RATE * PRESENTER_AUDIO_OUT_BUF_SIZE / PRESENTER_AUDIO_OUT_SAMPLE_RATE;

pub struct SoundSampler {
    queues: [(HeapArrayU32<SAMPLE_BUFFER_SIZE>, u16); 2],
    busy_queue: usize,
    ready_queue: usize,
    waiting: bool,
    busy: AtomicBool,
    sound_touch: SoundTouch,
    last_sample: u32,
    stretch_ratio: f32,
    average_size: f32,
    size_count: f32,
    cond_mutex: Mutex<bool>,
    condvar: Condvar,
}

impl SoundSampler {
    pub fn new() -> SoundSampler {
        let mut sound_touch = SoundTouch::new();
        sound_touch.set_channels(2);
        sound_touch.set_sample_rate(SAMPLE_RATE);
        sound_touch.set_pitch(1.0);
        sound_touch.set_tempo(1.0);
        SoundSampler {
            queues: [(HeapArrayU32::default(), 0), (HeapArrayU32::default(), 0)],
            busy_queue: 0,
            ready_queue: 0,
            waiting: false,
            busy: AtomicBool::new(false),
            sound_touch,
            last_sample: 0,
            stretch_ratio: 1.0,
            average_size: 0.0,
            size_count: 0.0,
            cond_mutex: Mutex::new(false),
            condvar: Condvar::new(),
        }
    }

    pub fn init(&mut self) {
        self.busy_queue = 0;
        self.ready_queue = 0;
        self.waiting = false;
        self.busy.store(false, Ordering::SeqCst);
        self.sound_touch.clear();
        self.last_sample = 0;
        self.stretch_ratio = 1.0;
        self.average_size = 0.0;
        self.size_count = 0.0;
        *self.cond_mutex.lock().unwrap() = false;
    }

    fn push(&mut self, sample: u32, framelimit: bool, audio_stretching: bool) {
        while self.busy.compare_exchange(false, true, Ordering::SeqCst, Ordering::Acquire).is_err() {}

        unsafe { assert_unchecked(self.busy_queue <= 1) };
        let (queue, size) = &mut self.queues[self.busy_queue];
        unsafe { assert_unchecked((*size as usize) < queue.len()) };
        queue[*size as usize] = sample;
        *size += 1;
        if *size == SAMPLE_BUFFER_SIZE as u16 {
            let (_, other_size) = &mut self.queues[self.busy_queue ^ 1];

            if !audio_stretching {
                let mut can_sample = self.cond_mutex.lock().unwrap();
                *can_sample = true;
                self.condvar.notify_one();
            }

            if framelimit && *other_size == SAMPLE_BUFFER_SIZE as u16 {
                self.waiting = true;
                self.busy.store(false, Ordering::SeqCst);
                thread::park();
                return;
            } else {
                *other_size = 0;
                self.ready_queue = self.busy_queue;
                self.busy_queue ^= 1;
            }
        }

        self.busy.store(false, Ordering::SeqCst);
    }

    pub fn consume(&mut self, cpu_thread: &Thread, buf: &mut [u32; SAMPLE_BUFFER_SIZE], ret: &mut [u32; PRESENTER_AUDIO_OUT_BUF_SIZE], audio_stretching: bool) {
        if !audio_stretching {
            let can_sample = self.cond_mutex.lock().unwrap();
            let (mut can_sample, timeout_result) = self.condvar.wait_timeout_while(can_sample, Duration::from_millis(500), |can_sample| !*can_sample).unwrap();
            if timeout_result.timed_out() {
                ret.fill(0);
                return;
            }
            *can_sample = false;
        }

        while self.busy.compare_exchange(false, true, Ordering::SeqCst, Ordering::Acquire).is_err() {}

        let ready_queue = self.ready_queue;
        let (queue, queue_size) = &mut self.queues[ready_queue];
        let mut size = *queue_size as usize;
        unsafe { assert_unchecked(size <= SAMPLE_BUFFER_SIZE) };
        debug_assert!(audio_stretching || size == SAMPLE_BUFFER_SIZE);
        *queue_size = 0;
        buf[..size].copy_from_slice(&queue[..size]);
        self.ready_queue = self.busy_queue;

        if self.waiting {
            self.waiting = false;
            self.busy_queue = ready_queue;
            cpu_thread.unpark();
        }

        self.busy.store(false, Ordering::SeqCst);

        if audio_stretching {
            // Taken from https://github.com/dolphin-emu/dolphin/blob/b5be399fd4175eb6c4ba83201bd4866b357b3200/Source/Core/AudioCommon/AudioStretcher.cpp#L28-L65
            // Take an average ratio so tempo doesn't change abruptly
            self.average_size += size as f32;
            self.size_count += 1.0;
            let ratio = self.average_size as f32 / self.size_count as f32 / SAMPLE_BUFFER_SIZE as f32;
            if self.size_count >= 15.0 {
                self.size_count = 0.0;
                self.average_size = 0.0;
            }
            // 80ms latency
            let max_backlog = SAMPLE_RATE as f32 * 80.0 / 1000.0;
            let backlog_fullness = self.sound_touch.num_of_samples() as f32 / max_backlog;
            if backlog_fullness > 5.0 {
                size = 0;
            }

            // Plot the function for understanding
            // In a nutshell backlog is not at 50% => slow down
            // More aggressive slow down when sample size is small
            let tweak = 1.0 + 2.0 * (backlog_fullness - 0.5) * (1.0 - ratio);
            let current_ratio = ratio * tweak;

            // The fewer samples, the smaller the lpf gain
            // Most likely the next audio frame will have more samples
            // Thus don't let it influence the ratio too much
            const LPF_TIME_SCALE: f32 = 1.0;
            let lpf_gain = 1.0 - (-ratio / LPF_TIME_SCALE).exp();
            self.stretch_ratio += lpf_gain * (current_ratio - self.stretch_ratio);

            if self.stretch_ratio < 0.05 {
                self.stretch_ratio = 0.05;
            }
            self.sound_touch.set_tempo(self.stretch_ratio as f64);

            let sound_touch_buf = unsafe { slice::from_raw_parts(buf.as_ptr() as *const i16, size << 1) };
            self.sound_touch.put_samples(sound_touch_buf, size);
            let sound_touch_buf = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut i16, SAMPLE_BUFFER_SIZE << 1) };
            let num_samples = self.sound_touch.receive_samples(sound_touch_buf, SAMPLE_BUFFER_SIZE);
            if num_samples == 0 {
                buf.fill(self.last_sample);
            } else if num_samples < SAMPLE_BUFFER_SIZE {
                self.last_sample = buf[num_samples - 1];
                buf[num_samples..].fill(self.last_sample);
            }
        }

        for i in 0..PRESENTER_AUDIO_OUT_BUF_SIZE {
            ret[i] = unsafe { *buf.get_unchecked(i * SAMPLE_BUFFER_SIZE / PRESENTER_AUDIO_OUT_BUF_SIZE) };
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
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum SoundChannelFormat {
    #[default]
    Pcm8 = 0,
    Pcm16 = 1,
    ImaAdpcm = 2,
    PsgNoise = 3,
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
    channels: [SpuChannel; CHANNEL_COUNT],
    sound_cap_channels: [SoundCapChannel; 2],
    main_sound_cnt: MainSoundCnt,
    sound_bias: u16,
    duty_cycles: [i32; 6],
    noise_values: [u16; 2],
    sound_sampler: NonNull<SoundSampler>,
}

impl Spu {
    pub fn new(sound_sampler: NonNull<SoundSampler>) -> Self {
        Spu {
            channels: [SpuChannel::default(); CHANNEL_COUNT],
            sound_cap_channels: [SoundCapChannel::default(); 2],
            main_sound_cnt: MainSoundCnt::from(0),
            sound_bias: 0,
            duty_cycles: [0; 6],
            noise_values: [0; 2],
            sound_sampler,
        }
    }

    pub fn init(&mut self) {
        self.channels = [SpuChannel::default(); CHANNEL_COUNT];
        self.sound_cap_channels = [SoundCapChannel::default(); 2];
        self.main_sound_cnt = MainSoundCnt::from(0);
        self.sound_bias = 0;
        self.duty_cycles = [0; 6];
        self.noise_values = [0; 2];
    }
}

impl Emu {
    pub fn spu_initialize_schedule(&mut self) {
        self.cm.schedule(512 * 2, EventType::SpuSample);
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

        debug_println!("spu set cnt {channel_num} {:x}", u32::from(channel.cnt));

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

        debug_println!("spu set sad {channel_num} {:x}", channel.sad);

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
        debug_println!("spu set tmr {channel_num} {:x}", self.spu.channels[channel_num].tmr);
    }

    pub fn spu_set_pnt(&mut self, channel_num: usize, mask: u16, value: u16) {
        self.spu.channels[channel_num].pnt = (self.spu.channels[channel_num].pnt & !mask) | (value & mask);
        debug_println!("spu set pnt {channel_num} {:x}", self.spu.channels[channel_num].pnt);
    }

    pub fn spu_set_len(&mut self, channel_num: usize, mut mask: u32, value: u32) {
        mask &= 0x003FFFFF;
        self.spu.channels[channel_num].len = (self.spu.channels[channel_num].len & !mask) | (value & mask);
        debug_println!("spu set len {channel_num} {:x}", self.spu.channels[channel_num].len);
    }

    pub fn spu_set_main_sound_cnt(&mut self, mut mask: u16, value: u16) {
        let was_disabled = !self.spu.main_sound_cnt.master_enable();

        mask &= 0xBF7F;
        self.spu.main_sound_cnt = ((u16::from(self.spu.main_sound_cnt) & !mask) | (value & mask)).into();

        debug_println!("spu set main sound cnt {:x}", u16::from(self.spu.main_sound_cnt));

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
        debug_println!("spu set sound bias {:x}", self.spu.sound_bias);
    }

    pub fn spu_set_snd_cap_cnt(&mut self, channel_num: usize, value: u8) {
        let cap_channel = &mut self.spu.sound_cap_channels[channel_num];
        let cnt = SoundCapCnt::from(value & 0x8F);
        if !cap_channel.cnt.start_status() && cnt.start_status() {
            cap_channel.dad_current = cap_channel.dad;
            cap_channel.tmr_current = self.spu.channels[(channel_num << 1) + 1].tmr as u32;
        }

        cap_channel.cnt = cnt;

        debug_println!("spu set snd cap cnt {:x}", u8::from(cap_channel.cnt));
    }

    pub fn spu_set_snd_cap_dad(&mut self, channel_num: usize, mut mask: u32, value: u32) {
        mask &= 0x07FFFFFC;
        let cap_channel = &mut self.spu.sound_cap_channels[channel_num];
        cap_channel.dad = (cap_channel.dad & !mask) | (value & mask);

        cap_channel.dad_current = cap_channel.dad;
        cap_channel.tmr_current = self.spu.channels[(channel_num << 1) + 1].tmr as u32;

        debug_println!("spu set snd cap cnt {:x}", u8::from(cap_channel.cnt));
    }

    pub fn spu_set_snd_cap_len(&mut self, channel_num: usize, mask: u16, value: u16) {
        let cap_channel = &mut self.spu.sound_cap_channels[channel_num];
        cap_channel.len = (cap_channel.len & !mask) | (value & mask);
        debug_println!("spu set snd cap len {:x}", cap_channel.len);
    }

    fn spu_start_channel(&mut self, channel_num: usize) {
        debug_println!("spu start channel {channel_num}");
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

        if channel.sad_current == channel.sad + ((channel.pnt as u32) << 2) && !channel.adpcm_toggle {
            channel.adpcm_loop_value = channel.adpcm_value;
            channel.adpcm_loop_index = channel.adpcm_index;
        }

        let sad_current = channel.sad_current;
        let adpcm_data = self.mem_read::<{ ARM7 }, u8>(sad_current);

        let channel = &mut self.spu.channels[channel_num];
        let adpcm_data = if channel.adpcm_toggle { adpcm_data >> 4 } else { adpcm_data & 0xF };

        let diff = unsafe { *ADPCM_DIFF_TABLE.get_unchecked(channel.adpcm_index as usize).get_unchecked(adpcm_data as usize) };
        channel.adpcm_value = (channel.adpcm_value as i32 + diff).clamp(-0x8000, 0x7FFF) as i16;

        channel.adpcm_index = unsafe { ADPCM_INDEX_TABLE.get_unchecked(channel.adpcm_index as usize)[(adpcm_data & 0x7) as usize] };

        channel.sad_current += channel.adpcm_toggle as u32;
        channel.adpcm_toggle = !channel.adpcm_toggle;
    }

    fn spu_sample_psg_noise(&self, channel_num: usize) -> i32 {
        unsafe { assert_unchecked(channel_num < CHANNEL_COUNT) };
        if channel_num >= 8 && channel_num <= 13 {
            let duty = 7 - u8::from(self.spu.channels[channel_num].cnt.wave_duty());
            if self.spu.duty_cycles[channel_num - 8] < duty as i32 {
                -0x7FFF
            } else {
                0x7FFF
            }
        } else if channel_num >= 14 {
            if (self.spu.noise_values[channel_num - 14] & (1 << 15)) != 0 {
                -0x7FFF
            } else {
                0x7FFF
            }
        } else {
            0
        }
    }

    pub fn spu_on_sample_event(&mut self) {
        if unlikely(!self.settings.audio()) {
            for i in 0..CHANNEL_COUNT {
                self.spu.channels[i].cnt.set_start_status(false);
            }
            for i in 0..2 {
                self.spu.sound_cap_channels[i].cnt.set_start_status(false);
            }
            unsafe { self.spu.sound_sampler.as_mut().push(0, self.settings.framelimit(), self.settings.audio_stretching()) };
            self.cm.schedule(512 * 2, EventType::SpuSample);
            return;
        }

        let mut mixer_left = 0;
        let mut mixer_right = 0;
        let mut channels_left = [0; 2];
        let mut channels_right = [0; 2];

        for i in 0..CHANNEL_COUNT {
            if !self.spu.channels[i].active {
                continue;
            }

            let channel = &mut self.spu.channels[i];
            let sad_current = channel.sad_current;
            let format = channel.cnt.get_format();

            let mut data = match format {
                SoundChannelFormat::Pcm8 => (self.mem_read::<{ ARM7 }, u8>(sad_current) as i8 as i32) << 8,
                SoundChannelFormat::Pcm16 => self.mem_read::<{ ARM7 }, u16>(sad_current) as i16 as i32,
                SoundChannelFormat::ImaAdpcm => channel.adpcm_value as i32,
                SoundChannelFormat::PsgNoise => self.spu_sample_psg_noise(i),
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
            self.spu.sound_sampler.as_mut().push(
                ((sample_right << 16) & 0xFFFF0000) | (sample_left & 0xFFFF),
                self.settings.framelimit(),
                self.settings.audio_stretching(),
            )
        };
        self.cm.schedule(512 * 2, EventType::SpuSample);
    }
}

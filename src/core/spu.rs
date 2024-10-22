use crate::core::cycle_manager::{CycleManager, EventType};
use crate::core::emu::{get_cm_mut, get_spu, get_spu_mut, Emu};
use crate::core::CpuType::ARM7;
use crate::presenter::{PRESENTER_AUDIO_BUF_SIZE, PRESENTER_AUDIO_SAMPLE_RATE};
use crate::utils::HeapMemU32;
use bilge::prelude::*;
use std::cmp::{max, min};
use std::collections::VecDeque;
use std::hint::unreachable_unchecked;
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

const ADPCM_INDEX_TABLE: [i8; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

const ADPCM_TABLE: [i16; 89] = [
    0x0007, 0x0008, 0x0009, 0x000A, 0x000B, 0x000C, 0x000D, 0x000E, 0x0010, 0x0011, 0x0013, 0x0015, 0x0017, 0x0019, 0x001C, 0x001F, 0x0022, 0x0025, 0x0029, 0x002D, 0x0032, 0x0037, 0x003C, 0x0042,
    0x0049, 0x0050, 0x0058, 0x0061, 0x006B, 0x0076, 0x0082, 0x008F, 0x009D, 0x00AD, 0x00BE, 0x00D1, 0x00E6, 0x00FD, 0x0117, 0x0133, 0x0151, 0x0173, 0x0198, 0x01C1, 0x01EE, 0x0220, 0x0256, 0x0292,
    0x02D4, 0x031C, 0x036C, 0x03C3, 0x0424, 0x048E, 0x0502, 0x0583, 0x0610, 0x06AB, 0x0756, 0x0812, 0x08E0, 0x09C3, 0x0ABD, 0x0BD0, 0x0CFF, 0x0E4C, 0x0FBA, 0x114C, 0x1307, 0x14EE, 0x1706, 0x1954,
    0x1BDC, 0x1EA5, 0x21B6, 0x2515, 0x28CA, 0x2CDF, 0x315B, 0x364B, 0x3BB9, 0x41B2, 0x4844, 0x4F7E, 0x5771, 0x602F, 0x69CE, 0x7462, 0x7FFF,
];

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
pub struct SoundCnt {
    pub volume_mul: u7,
    not_used: u1,
    pub volume_div: u2,
    not_used1: u5,
    pub hold: u1,
    pub panning: u7,
    not_used2: u1,
    pub wave_duty: u3,
    pub repeat_mode: u2,
    pub format: u2,
    pub start_status: u1,
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
    pub output_ch1_to_mixer: u1,
    pub output_ch3_to_mixer: u1,
    not_used2: u1,
    pub master_enable: u1,
}

#[derive(Copy, Clone, Default)]
pub struct SpuChannel {
    cnt: SoundCnt,
    volume: u8,
    sad: u32,
    tmr: u16,
    pnt: u16,
    len: u32,
    snd_cap_cnt: u8,
    sad_current: u32,
    tmr_current: u32,
    adpcm_value: i32,
    adpcm_loop_value: i32,
    adpcm_index: i32,
    adpcm_loop_index: i32,
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
#[derive(FromBits)]
pub struct SoundCapCnt {
    pub cnt_associated_channels: u1,
    pub cap_src_select: u1,
    pub cap_repeat: u1,
    pub cap_format: u1,
    not_used: u3,
    pub cap_start_status: u1,
}

pub struct Spu {
    pub audio_enabled: bool,
    pub channels: [SpuChannel; CHANNEL_COUNT],
    main_sound_cnt: MainSoundCnt,
    master_volume: u8,
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
            main_sound_cnt: MainSoundCnt::from(0),
            master_volume: 0,
            sound_bias: 0,
            duty_cycles: [0; 6],
            noise_values: [0; 2],
            samples_buffer: Vec::new(),
            sound_sampler,
        }
    }

    pub fn initialize_schedule(cycle_manager: &mut CycleManager) {
        cycle_manager.schedule(512 * 2, EventType::SpuSample);
    }

    pub fn get_cnt(&self, channel: usize) -> u32 {
        self.channels[channel].cnt.into()
    }

    pub fn get_main_sound_cnt(&self) -> u16 {
        self.main_sound_cnt.into()
    }

    pub fn get_snd_cap_cnt(&self, channel: usize) -> u8 {
        self.channels[channel].snd_cap_cnt
    }

    pub fn set_cnt(&mut self, channel: usize, mut mask: u32, value: u32, emu: &mut Emu) {
        let was_disabled = !bool::from(self.channels[channel].cnt.start_status());

        mask &= 0xFF7F837F;
        self.channels[channel].cnt = ((u32::from(self.channels[channel].cnt) & !mask) | (value & mask)).into();
        let mut volume = u8::from(self.channels[channel].cnt.volume_mul());
        if volume == 127 {
            volume += 1;
        }
        self.channels[channel].volume = volume;

        if was_disabled
            && bool::from(self.channels[channel].cnt.start_status())
            && bool::from(self.main_sound_cnt.master_enable())
            && (self.channels[channel].sad != 0 || self.channels[channel].cnt.get_format() == SoundChannelFormat::PsgNoise)
        {
            self.start_channel(channel, emu);
        } else if !bool::from(self.channels[channel].cnt.start_status()) {
            self.channels[channel].active = false;
        }
    }

    pub fn set_sad(&mut self, channel: usize, mut mask: u32, value: u32, emu: &mut Emu) {
        mask &= 0x07FFFFFC;
        self.channels[channel].sad = (self.channels[channel].sad & !mask) | (value & mask);

        if self.channels[channel].cnt.get_format() != SoundChannelFormat::PsgNoise {
            if self.channels[channel].sad != 0 && (bool::from(self.main_sound_cnt.master_enable()) && bool::from(self.channels[channel].cnt.start_status())) {
                self.start_channel(channel, emu);
            } else {
                self.channels[channel].active = false;
            }
        }
    }

    pub fn set_tmr(&mut self, channel: usize, mask: u16, value: u16) {
        self.channels[channel].tmr = (self.channels[channel].tmr & !mask) | (value & mask);
    }

    pub fn set_pnt(&mut self, channel: usize, mask: u16, value: u16) {
        self.channels[channel].pnt = (self.channels[channel].pnt & !mask) | (value & mask);
    }

    pub fn set_len(&mut self, channel: usize, mut mask: u32, value: u32) {
        mask &= 0x003FFFFF;
        self.channels[channel].len = (self.channels[channel].len & !mask) | (value & mask);
    }

    pub fn set_main_sound_cnt(&mut self, mut mask: u16, value: u16, emu: &mut Emu) {
        let was_disabled = !bool::from(self.main_sound_cnt.master_enable());

        mask &= 0xBF7F;
        self.main_sound_cnt = ((u16::from(self.main_sound_cnt) & !mask) | (value & mask)).into();
        let mut volume = u8::from(self.main_sound_cnt.master_volume());
        if volume == 127 {
            volume += 1;
        }
        self.master_volume = volume;

        if was_disabled && bool::from(self.main_sound_cnt.master_enable()) {
            for i in 0..CHANNEL_COUNT {
                if bool::from(self.channels[i].cnt.start_status()) && (self.channels[i].sad != 0 || self.channels[i].cnt.get_format() == SoundChannelFormat::PsgNoise) {
                    self.start_channel(i, emu);
                }
            }
        } else if !bool::from(self.main_sound_cnt.master_enable()) {
            for channel in &mut self.channels {
                channel.active = false;
            }
        }
    }

    pub fn set_sound_bias(&mut self, mut mask: u16, value: u16) {
        mask &= 0x03FF;
        self.sound_bias = (self.sound_bias & !mask) | (value & mask);
    }

    pub fn set_snd_cap_cnt(&mut self, channel: usize, value: u8) {
        // TODO
    }

    pub fn set_snd_cap_dad(&mut self, channel: usize, mask: u32, value: u32) {
        // TODO
    }

    pub fn set_snd_cap_len(&mut self, channel: usize, mask: u16, value: u16) {
        // TODO
    }

    fn start_channel(&mut self, channel_num: usize, emu: &mut Emu) {
        let channel = &mut self.channels[channel_num];
        channel.sad_current = channel.sad;
        channel.tmr_current = channel.tmr as u32;

        match channel.cnt.get_format() {
            SoundChannelFormat::ImaAdpcm => {
                let header = AdpcmHeader::from(emu.mem_read::<{ ARM7 }, u32>(channel.sad_current));
                channel.adpcm_value = header.pcm16_value() as i16 as i32;
                channel.adpcm_index = min(u8::from(header.table_index()) as i32, 88);
                channel.adpcm_toggle = false;
                channel.sad_current += 4;
            }
            SoundChannelFormat::PsgNoise => {
                if (8..=13).contains(&channel_num) {
                    self.duty_cycles[channel_num - 8] = 0;
                } else if channel_num >= 14 {
                    self.noise_values[channel_num - 14] = 0x7FFF;
                }
            }
            _ => {}
        }

        channel.active = true;
    }

    fn next_sample_pcm(channel: &mut SpuChannel) {
        channel.sad_current += 1 + u8::from(channel.cnt.format()) as u32;
    }

    fn next_sample_adpcm(channel: &mut SpuChannel, emu: &mut Emu) {
        if channel.sad_current == channel.sad + ((channel.pnt as u32) << 2) && !channel.adpcm_toggle {
            channel.adpcm_loop_value = channel.adpcm_value;
            channel.adpcm_loop_index = channel.adpcm_index;
        }

        let sad_current = channel.sad_current;
        let adpcm_data = emu.mem_read::<{ ARM7 }, u8>(sad_current);

        let adpcm_data = if channel.adpcm_toggle { adpcm_data >> 4 } else { adpcm_data & 0xF };

        let mut diff = (ADPCM_TABLE[channel.adpcm_index as usize] / 8) as i32;
        if adpcm_data & 1 != 0 {
            diff += (ADPCM_TABLE[channel.adpcm_index as usize] / 4) as i32;
        }
        if adpcm_data & 2 != 0 {
            diff += (ADPCM_TABLE[channel.adpcm_index as usize] / 2) as i32;
        }
        if adpcm_data & 4 != 0 {
            diff += ADPCM_TABLE[channel.adpcm_index as usize] as i32;
        }

        if adpcm_data & 8 != 0 {
            channel.adpcm_value += diff;
            channel.adpcm_value = min(channel.adpcm_value, 0x7FFF);
        } else {
            channel.adpcm_value -= diff;
            channel.adpcm_value = max(channel.adpcm_value, -0x7FFF);
        }

        channel.adpcm_index += ADPCM_INDEX_TABLE[(adpcm_data & 0x7) as usize] as i32;
        channel.adpcm_index = channel.adpcm_index.clamp(0, 88);

        channel.adpcm_toggle = !channel.adpcm_toggle;
        if !channel.adpcm_toggle {
            channel.sad_current += 1;
        }
    }

    fn next_sample_psg(&mut self, channel_num: usize) {
        if (8..=13).contains(&channel_num) {
            self.duty_cycles[channel_num - 8] = (self.duty_cycles[channel_num - 8] + 1) % 8;
        } else if channel_num >= 14 {
            self.noise_values[channel_num - 14] &= !(1 << 15);
            if self.noise_values[channel_num - 14] & 1 != 0 {
                self.noise_values[channel_num - 14] = (1 << 15) | ((self.noise_values[channel_num - 14] >> 1) ^ 0x6000);
            } else {
                self.noise_values[channel_num - 14] >>= 1;
            }
        }
    }

    pub fn on_sample_event(emu: &mut Emu) {
        let mut mixer_left = 0;
        let mut mixer_right = 0;
        let mut channels_left = [0; 2];
        let mut channels_right = [0; 2];

        macro_rules! get_channel {
            ($emu:expr, $channel:expr) => {{
                &get_spu!($emu).channels[$channel]
            }};
        }

        macro_rules! get_channel_mut {
            ($emu:expr, $channel:expr) => {{
                &mut get_spu_mut!($emu).channels[$channel]
            }};
        }

        if unlikely(!get_spu!(emu).audio_enabled) {
            for i in 0..CHANNEL_COUNT {
                get_channel_mut!(emu, i).cnt.set_start_status(u1::new(0));
            }
            let spu = get_spu_mut!(emu);
            spu.samples_buffer.push(0);
            if unlikely(spu.samples_buffer.len() == SAMPLE_BUFFER_SIZE) {
                spu.sound_sampler.push(spu.samples_buffer.as_slice());
                spu.samples_buffer.clear();
            }
            get_cm_mut!(emu).schedule(512 * 2, EventType::SpuSample);
            return;
        }

        for i in 0..CHANNEL_COUNT {
            if likely(!get_channel!(emu, i).active) {
                continue;
            }

            let format = get_channel!(emu, i).cnt.get_format();

            let mut data = match format {
                SoundChannelFormat::Pcm8 => (emu.mem_read::<{ ARM7 }, u8>(get_channel!(emu, i).sad_current) as i8 as i64) << 8,
                SoundChannelFormat::Pcm16 => emu.mem_read::<{ ARM7 }, u16>(get_channel!(emu, i).sad_current) as i16 as i64,
                SoundChannelFormat::ImaAdpcm => get_channel!(emu, i).adpcm_value as i64,
                SoundChannelFormat::PsgNoise => {
                    if i >= 8 && i <= 13 {
                        let duty = 7 - u8::from(get_channel!(emu, i).cnt.wave_duty());
                        if get_spu!(emu).duty_cycles[i - 8] < duty as i32 {
                            -0x7FFF
                        } else {
                            0x7FFF
                        }
                    } else if i >= 14 {
                        if (get_spu!(emu).noise_values[i - 14] & (1 << 15)) != 0 {
                            -0x7FFF
                        } else {
                            0x7FFF
                        }
                    } else {
                        0
                    }
                }
            };

            let mut tmr_current = get_channel!(emu, i).tmr_current.wrapping_add(512);
            let tmr = get_channel!(emu, i).tmr;
            while tmr_current >> 16 != 0 {
                tmr_current = tmr as u32 + (tmr_current - 0x10000);

                match format {
                    SoundChannelFormat::Pcm8 | SoundChannelFormat::Pcm16 => Self::next_sample_pcm(get_channel_mut!(emu, i)),
                    SoundChannelFormat::ImaAdpcm => Self::next_sample_adpcm(get_channel_mut!(emu, i), emu),
                    SoundChannelFormat::PsgNoise => get_spu_mut!(emu).next_sample_psg(i),
                }

                let channel = get_channel_mut!(emu, i);
                if format != SoundChannelFormat::PsgNoise && channel.sad_current >= (channel.sad + ((channel.pnt as u32 + channel.len) << 2)) {
                    if u8::from(channel.cnt.repeat_mode()) == 1 {
                        channel.sad_current = channel.sad + ((channel.pnt as u32) << 2);

                        if format == SoundChannelFormat::ImaAdpcm {
                            channel.adpcm_value = channel.adpcm_loop_value;
                            channel.adpcm_index = channel.adpcm_loop_index;
                            channel.adpcm_toggle = false;
                        }
                    } else {
                        channel.cnt.set_start_status(u1::new(0));
                        channel.active = false;
                        break;
                    }
                }
            }
            get_channel_mut!(emu, i).tmr_current = tmr_current;

            let channel = get_channel!(emu, i);
            let volume_div = u8::from(channel.cnt.volume_div());
            const VOLUME_DIVS: [u8; 4] = [4, 3, 2, 0];
            data <<= VOLUME_DIVS[volume_div as usize];

            let volume_mul = channel.volume as i64;
            data = ((data << 7) * volume_mul) >> 7;

            let mut panning = u8::from(channel.cnt.panning()) as i64;
            if panning == 127 {
                panning += 1;
            }
            let data_left = (data * (128 - panning) / 128) >> 3;
            let data_right = (data * panning / 128) >> 3;

            if i == 1 {
                channels_left[0] = data_left;
                channels_right[0] = data_right;
                if bool::from(get_spu!(emu).main_sound_cnt.output_ch1_to_mixer()) {
                    continue;
                }
            } else if i == 3 {
                channels_left[1] = data_left;
                channels_right[1] = data_right;
                if bool::from(get_spu!(emu).main_sound_cnt.output_ch3_to_mixer()) {
                    continue;
                }
            }

            mixer_left += data_left;
            mixer_right += data_right;
        }

        let spu = get_spu!(emu);

        let sample_left = match u8::from(spu.main_sound_cnt.left_output_from()) {
            0 => mixer_left,
            1 => channels_left[0],
            2 => channels_left[1],
            3 => channels_left[0] + channels_left[1],
            _ => unsafe { unreachable_unchecked() },
        };

        let sample_right = match u8::from(spu.main_sound_cnt.right_output_from()) {
            0 => mixer_right,
            1 => channels_right[0],
            2 => channels_right[1],
            3 => channels_right[0] + channels_right[1],
            _ => unsafe { unreachable_unchecked() },
        };

        let master_volume = spu.master_volume as i64;
        let sample_left = ((sample_left * master_volume) >> 7) >> 8;
        let sample_right = ((sample_right * master_volume) >> 7) >> 8;

        let sample_left = (sample_left >> 6) + spu.sound_bias as i64;
        let sample_right = (sample_right >> 6) + spu.sound_bias as i64;

        let sample_left = sample_left.clamp(0, 0x3FF);
        let sample_right = sample_right.clamp(0, 0x3FF);

        let sample_left = (sample_left - 0x200) << 5;
        let sample_right = (sample_right - 0x200) << 5;

        let spu = get_spu_mut!(emu);
        spu.samples_buffer.push(((sample_right << 16) | (sample_left & 0xFFFF)) as u32);
        if unlikely(spu.samples_buffer.len() == SAMPLE_BUFFER_SIZE) {
            spu.sound_sampler.push(spu.samples_buffer.as_slice());
            spu.samples_buffer.clear();
        }

        get_cm_mut!(emu).schedule(512 * 2, EventType::SpuSample);
    }
}

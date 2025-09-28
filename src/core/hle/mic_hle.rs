use crate::core::cycle_manager::EventType;
use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::IpcFifoTag;
use crate::core::spi::MIC_SAMPLE_CYCLES;
use crate::core::CpuType::ARM7;
use crate::presenter::PRESENTER_AUDIO_IN_SAMPLE_RATE;
use bilge::prelude::*;

#[bitsize(8)]
#[derive(Copy, Clone, FromBits)]
pub struct SampleFlags {
    is_16bit: bool,
    signed: bool,
    unused: u2,
    repeat: bool,
    unsued2: u3,
}

impl SampleFlags {
    fn adjust_sample(self, sample: i16) -> u16 {
        if self.is_16bit() {
            if self.signed() {
                sample as u16
            } else {
                (sample as i32 + 0x8000) as i16 as u16
            }
        } else if self.signed() {
            (sample >> 8) as i8 as u16
        } else {
            ((sample >> 8) + 0x80) as u8 as u16
        }
    }
}

pub(super) struct MicHle {
    data: [u16; 16],
    sample_flags: SampleFlags,
    sample_buf: u32,
    sample_size: u32,
    sample_max_count: u32,
    sample_count: u32,
}

impl MicHle {
    pub(super) fn new() -> Self {
        MicHle {
            data: [0; 16],
            sample_flags: SampleFlags::from(0),
            sample_buf: 0,
            sample_size: 0,
            sample_max_count: 0,
            sample_count: 0,
        }
    }
}

impl Emu {
    pub(super) fn mic_hle_ipc_recv(&mut self, data: u32) {
        let mic = &mut self.hle.mic;

        if data & (1 << 25) != 0 {
            mic.data.fill(0);
        }

        mic.data[((data >> 16) & 0xF) as usize] = data as u16;

        if (data & (1 << 24)) == 0 {
            return;
        }

        let cmd = mic.data[0] >> 8;
        match cmd {
            0x40 => {
                let sample_type = (mic.data[0] as u8) & 0x3;
                let sample_flags = SampleFlags::from(sample_type);
                let sample = sample_flags.adjust_sample(self.spi_mic_sample());
                if sample_flags.is_16bit() {
                    self.mem_write::<{ ARM7 }, u16>(0x27fff94, sample);
                } else {
                    self.mem_write::<{ ARM7 }, u8>(0x27fff94, sample as u8);
                }

                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Mic, 0x300c000, false);
            }
            0x41 => {
                let flags = mic.data[0] as u8;
                let buf = ((mic.data[1] as u32) << 16) | (mic.data[2] as u32);
                let size = ((mic.data[3] as u32) << 16) | (mic.data[4] as u32);
                let rate = 33513982 / (((mic.data[5] as u32) << 16) | (mic.data[6] as u32));
                mic.sample_flags = SampleFlags::from(flags);
                mic.sample_buf = buf;
                mic.sample_size = size >> mic.sample_flags.is_16bit() as u8;
                mic.sample_count = 0;
                let sample_max_count = mic.sample_size as u64 * PRESENTER_AUDIO_IN_SAMPLE_RATE as u64 / rate as u64;
                mic.sample_max_count = sample_max_count as u32;

                self.cm.schedule(MIC_SAMPLE_CYCLES, EventType::MicSampleHle);
                self.mem_write::<{ ARM7 }, u32>(0x27fff90, 0);

                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Mic, 0x300c100, false);
            }
            0x42 => {
                mic.sample_size = 0;

                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Mic, 0x300c200, false);
            }
            0x43 => {
                let rate = 33513982 / (((mic.data[1] as u32) << 16) | (mic.data[2] as u32));
                let sample_max_count = mic.sample_size as u64 * PRESENTER_AUDIO_IN_SAMPLE_RATE as u64 / rate as u64;
                mic.sample_count = 0;
                mic.sample_max_count = sample_max_count as u32;

                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Mic, 0x300c300, false);
            }
            _ => println!("unknown mic request {data:x}"),
        }
    }

    pub fn mic_hle_sample_event(&mut self) {
        if self.hle.mic.sample_size == 0 {
            return;
        }

        let index = self.hle.mic.sample_count as usize % self.spi.mic_samples.len();
        if index == 0 {
            let mut mic_sampler = self.spi.mic_sampler.lock().unwrap();
            mic_sampler.consume(&mut self.spi.mic_samples);
        }
        let sample = self.hle.mic.sample_flags.adjust_sample(self.spi.mic_samples[index]);
        let addr = self.hle.mic.sample_buf + ((self.hle.mic.sample_count * self.hle.mic.sample_size / self.hle.mic.sample_max_count) << self.hle.mic.sample_flags.is_16bit() as u8);
        if self.hle.mic.sample_flags.is_16bit() {
            self.mem_write::<{ ARM7 }, u16>(addr, sample);
            self.mem_write::<{ ARM7 }, u16>(0x27fff94, sample);
        } else {
            self.mem_write::<{ ARM7 }, u8>(addr, sample as u8);
            self.mem_write::<{ ARM7 }, u8>(0x27fff94, sample as u8);
        }
        self.mem_write::<{ ARM7 }, u32>(0x27fff90, addr);

        self.hle.mic.sample_count += 1;
        if self.hle.mic.sample_count == self.hle.mic.sample_max_count {
            self.arm7_hle_send_ipc_fifo(IpcFifoTag::Mic, 0x300d100, false);
            if self.hle.mic.sample_flags.repeat() {
                self.hle.mic.sample_count = 0;
                self.cm.schedule(MIC_SAMPLE_CYCLES, EventType::MicSampleHle);
            }
        } else {
            self.cm.schedule(MIC_SAMPLE_CYCLES, EventType::MicSampleHle);
        }
    }
}

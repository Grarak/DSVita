use crate::core::cycle_manager::EventType::{Overflow, SoundAlarm0Hle, Timer0Arm9};
use crate::core::cycle_manager::ImmEventType::{CartridgeWordReadArm9, CpuInterruptArm9, Dma0Arm9, Dma3Arm7};
use crate::core::emu::Emu;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use std::cmp::max;
use std::intrinsics::{likely, unlikely};
use std::mem;

#[repr(u8)]
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum ImmEventType {
    CpuInterruptArm9 = 0,
    CpuInterruptArm7 = 1,
    CartridgeWordReadArm9 = 2,
    CartridgeWordReadArm7 = 3,
    Dma0Arm9 = 4,
    Dma0Arm7 = 5,
    Dma1Arm9 = 6,
    Dma1Arm7 = 7,
    Dma2Arm9 = 8,
    Dma2Arm7 = 9,
    Dma3Arm9 = 10,
    Dma3Arm7 = 11,
}

impl ImmEventType {
    pub fn cpu_interrupt(cpu: CpuType) -> Self {
        ImmEventType::from(CpuInterruptArm9 as u8 + cpu as u8)
    }

    pub fn cartridge_word_read(cpu: CpuType) -> Self {
        ImmEventType::from(CartridgeWordReadArm9 as u8 + cpu as u8)
    }

    pub fn dma(cpu: CpuType, channel_num: u8) -> Self {
        ImmEventType::from(Dma0Arm9 as u8 + (channel_num << 1) + cpu as u8)
    }
}

impl From<u8> for ImmEventType {
    fn from(value: u8) -> Self {
        debug_assert!(value <= Overflow as u8);
        unsafe { mem::transmute(value) }
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum EventType {
    GpuScanline256 = 0,
    GpuScanline355 = 1,
    SpuSample = 2,
    Timer0Arm9 = 3,
    Timer0Arm7 = 4,
    Timer1Arm9 = 5,
    Timer1Arm7 = 6,
    Timer2Arm9 = 7,
    Timer2Arm7 = 8,
    Timer3Arm9 = 9,
    Timer3Arm7 = 10,
    SoundCmdHle = 11,
    SoundAlarm0Hle = 12,
    SoundAlarm1Hle = 13,
    SoundAlarm2Hle = 14,
    SoundAlarm3Hle = 15,
    SoundAlarm4Hle = 16,
    SoundAlarm5Hle = 17,
    SoundAlarm6Hle = 18,
    SoundAlarm7Hle = 19,
    WifiScanHle = 20,
    MicSampleHle = 21,
    Overflow = 22,
}

impl EventType {
    pub fn timer(cpu: CpuType, channel_num: u8) -> Self {
        EventType::from(Timer0Arm9 as u8 + (channel_num << 1) + cpu as u8)
    }

    pub fn sound_alarm_hle(id: u8) -> Self {
        EventType::from(SoundAlarm0Hle as u8 + id)
    }
}

impl From<u8> for EventType {
    fn from(value: u8) -> Self {
        debug_assert!(value <= Overflow as u8);
        unsafe { mem::transmute(value) }
    }
}

pub struct CycleManager {
    cycle_count: u32,
    events: [u32; Overflow as usize + 1],
    next_event_cycle: u32,
    active_events: u32,
    active_imm_events: u32,
}

impl CycleManager {
    pub fn new() -> Self {
        CycleManager {
            cycle_count: 0,
            events: [0; Overflow as usize + 1],
            next_event_cycle: u32::MAX,
            active_events: 0,
            active_imm_events: 0,
        }
    }

    pub fn init(&mut self) {
        self.cycle_count = 0;
        self.events = [0; Overflow as usize + 1];
        self.next_event_cycle = u32::MAX;
        self.active_events = 0;
        self.active_imm_events = 0;
    }

    pub fn add_cycles(&mut self, cycle_count: u16) {
        self.cycle_count += cycle_count as u32;
    }

    pub fn get_cycles(&self) -> u32 {
        self.cycle_count
    }

    pub fn schedule_imm(&mut self, event_type: ImmEventType) {
        self.active_imm_events |= 1 << (31 - event_type as u8);
    }

    pub fn schedule(&mut self, in_cycles: u32, event_type: EventType) {
        let mut in_cycles = max(in_cycles, 1);
        if unlikely(u32::MAX - in_cycles < self.cycle_count) {
            in_cycles = u32::MAX - self.cycle_count;
        }
        let event_cycle = self.cycle_count + in_cycles;
        self.events[event_type as usize] = event_cycle;
        self.active_events |= 1 << (31 - event_type as u8);
        if event_cycle < self.next_event_cycle {
            self.next_event_cycle = event_cycle;
        }
    }

    pub fn jump_to_next_event(&mut self) {
        debug_assert!(self.cycle_count <= self.next_event_cycle);
        self.cycle_count = self.next_event_cycle;
    }
}

impl Emu {
    pub fn cm_check_events(&mut self) -> bool {
        const IMM_LUT: [fn(&mut Emu); Dma3Arm7 as usize + 1] = [
            Emu::cpu_on_interrupt_event::<{ ARM9 }>,
            Emu::cpu_on_interrupt_event::<{ ARM7 }>,
            Emu::cartridge_on_word_read_event::<{ ARM9 }>,
            Emu::cartridge_on_word_read_event::<{ ARM7 }>,
            Emu::dma_on_event0::<{ ARM9 }>,
            Emu::dma_on_event0::<{ ARM7 }>,
            Emu::dma_on_event1::<{ ARM9 }>,
            Emu::dma_on_event1::<{ ARM7 }>,
            Emu::dma_on_event2::<{ ARM9 }>,
            Emu::dma_on_event2::<{ ARM7 }>,
            Emu::dma_on_event3::<{ ARM9 }>,
            Emu::dma_on_event3::<{ ARM7 }>,
        ];

        const LUT: [fn(&mut Emu); Overflow as usize + 1] = [
            Emu::gpu_on_scanline256_event,
            Emu::gpu_on_scanline355_event,
            Emu::spu_on_sample_event,
            Emu::timers_on_overflow_event::<{ ARM9 }, 0>,
            Emu::timers_on_overflow_event::<{ ARM7 }, 0>,
            Emu::timers_on_overflow_event::<{ ARM9 }, 1>,
            Emu::timers_on_overflow_event::<{ ARM7 }, 1>,
            Emu::timers_on_overflow_event::<{ ARM9 }, 2>,
            Emu::timers_on_overflow_event::<{ ARM7 }, 2>,
            Emu::timers_on_overflow_event::<{ ARM9 }, 3>,
            Emu::timers_on_overflow_event::<{ ARM7 }, 3>,
            Emu::sound_nitro_on_cmd_event,
            Emu::sound_nitro_on_alarm_event::<0>,
            Emu::sound_nitro_on_alarm_event::<1>,
            Emu::sound_nitro_on_alarm_event::<2>,
            Emu::sound_nitro_on_alarm_event::<3>,
            Emu::sound_nitro_on_alarm_event::<4>,
            Emu::sound_nitro_on_alarm_event::<5>,
            Emu::sound_nitro_on_alarm_event::<6>,
            Emu::sound_nitro_on_alarm_event::<7>,
            Emu::wifi_hle_on_scan_event,
            Emu::mic_hle_sample_event,
            Emu::cm_on_overflow_event,
        ];

        let mut active_imm_events = self.cm.active_imm_events;
        self.cm.active_imm_events = 0;
        let mut offset = 0;
        while active_imm_events != 0 {
            let zeros = active_imm_events.leading_zeros();
            let event_index = (zeros + offset) as usize;

            let func = unsafe { IMM_LUT.get_unchecked(event_index) };
            func(self);

            active_imm_events <<= zeros + 1;
            offset += zeros + 1;
        }

        if likely(self.cm.cycle_count < self.cm.next_event_cycle) {
            return false;
        }

        let mut active_events = self.cm.active_events;
        self.cm.next_event_cycle = u32::MAX;
        let mut offset = 0;

        while {
            let zeros = active_events.leading_zeros();
            let event_index = (zeros + offset) as usize;

            let event_cycle = unsafe { *self.cm.events.get_unchecked(event_index) };
            if event_cycle <= self.cm.cycle_count {
                self.cm.active_events &= !(1 << (31 - event_index));
                let func = unsafe { LUT.get_unchecked(event_index) };
                func(self);
            } else if event_cycle < self.cm.next_event_cycle {
                self.cm.next_event_cycle = event_cycle;
            }

            active_events <<= zeros + 1;
            offset += zeros + 1;
            active_events != 0
        } {}
        true
    }

    fn cm_on_overflow_event(&mut self) {
        for i in 0..self.cm.events.len() {
            if self.cm.active_events & (1 << (31 - i)) != 0 {
                self.cm.events[i] -= self.cm.cycle_count;
            }
        }
        self.cm.next_event_cycle -= self.cm.cycle_count;
        for timer in &mut self.timers {
            for channel in &mut timer.channels {
                if channel.scheduled_cycle < self.cm.cycle_count {
                    channel.scheduled_cycle = 0;
                } else {
                    channel.scheduled_cycle -= self.cm.cycle_count;
                }
            }
        }
        if self.gpu.gpu_3d_regs.last_total_cycles < self.cm.cycle_count {
            self.gpu.gpu_3d_regs.last_total_cycles = 0;
        } else {
            self.gpu.gpu_3d_regs.last_total_cycles -= self.cm.cycle_count;
        }
        if self.spi.mic_sample_cycle < self.cm.cycle_count {
            self.spi.mic_sample_cycle = 0;
        } else {
            self.spi.mic_sample_cycle -= self.cm.cycle_count;
        }
        self.cm.cycle_count = 0;
        self.cm.schedule(0x7FFFFFFF, Overflow);
    }
}

use crate::emu::cpu_regs::CpuRegs;
use crate::emu::emu::Emu;
use crate::emu::gpu::gpu::Gpu;
use crate::emu::hle::sound_nitro::SoundNitro;
use crate::emu::memory::cartridge::Cartridge;
use crate::emu::memory::dma::Dma;
use crate::emu::spu::Spu;
use crate::emu::timers::Timers;
use crate::emu::CpuType::{ARM7, ARM9};
use std::collections::VecDeque;
use std::intrinsics::unlikely;

pub enum EventType {
    CpuInterruptArm9,
    CpuInterruptArm7,
    GpuScanline256,
    GpuScanline355,
    SoundCmdHle,
    SoundAlarmHle(u8),
    CartridgeWordReadArm9,
    CartridgeWordReadArm7,
    DmaArm9(u8),
    DmaArm7(u8),
    SpuSample,
    TimerArm9(u8),
    TimerArm7(u8),
}

pub struct CycleManager {
    pub cycle_count: u64,
    events: VecDeque<(u64, EventType)>,
}

impl CycleManager {
    pub fn new() -> Self {
        CycleManager {
            cycle_count: 0,
            events: VecDeque::new(),
        }
    }

    pub fn add_cycle(&mut self, cycles_to_add: u16) {
        self.cycle_count += cycles_to_add as u64;
    }

    #[inline(always)]
    pub fn check_events(&mut self, emu: &mut Emu) -> bool {
        let cycle_count = self.cycle_count;
        let mut event_triggered = false;
        while {
            let (cycles, _) = unsafe { self.events.front().unwrap_unchecked() };
            unlikely(*cycles <= cycle_count)
        } {
            event_triggered = true;
            let (cycles, event) = unsafe { self.events.pop_front().unwrap_unchecked() };
            match event {
                EventType::CpuInterruptArm9 => CpuRegs::on_interrupt_event::<{ ARM9 }>(emu),
                EventType::CpuInterruptArm7 => CpuRegs::on_interrupt_event::<{ ARM7 }>(emu),
                EventType::GpuScanline256 => Gpu::on_scanline256_event(emu),
                EventType::GpuScanline355 => Gpu::on_scanline355_event(emu),
                EventType::SoundCmdHle => SoundNitro::on_cmd_event(emu),
                EventType::SoundAlarmHle(id) => SoundNitro::on_alarm_event(id, emu),
                EventType::CartridgeWordReadArm9 => Cartridge::on_word_read_event::<{ ARM9 }>(emu),
                EventType::CartridgeWordReadArm7 => Cartridge::on_word_read_event::<{ ARM7 }>(emu),
                EventType::DmaArm9(channel) => Dma::on_event::<{ ARM9 }>(channel, emu),
                EventType::DmaArm7(channel) => Dma::on_event::<{ ARM7 }>(channel, emu),
                EventType::SpuSample => Spu::on_sample_event(emu),
                EventType::TimerArm9(channel) => Timers::on_overflow_event::<{ ARM9 }>(cycles, channel, emu),
                EventType::TimerArm7(channel) => Timers::on_overflow_event::<{ ARM7 }>(cycles, channel, emu),
            }
        }
        event_triggered
    }

    pub fn schedule(&mut self, in_cycles: u32, event_type: EventType) -> u64 {
        debug_assert_ne!(in_cycles, 0);
        let event_cycle = self.cycle_count + in_cycles as u64;
        let index = self.events.iter().position(|(cycles, _)| *cycles > event_cycle).unwrap_or(self.events.len());
        self.events.insert(index, (event_cycle, event_type));
        event_cycle
    }

    pub fn jump_to_next_event(&mut self) {
        self.cycle_count = unsafe { self.events.front().unwrap_unchecked().0 };
    }
}

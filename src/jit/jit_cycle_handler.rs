use crate::hle::CpuType;
use std::time::Duration;

struct AverageCycle {
    single_cycle_dur: Duration,
    dur_sum: u64,
    cycles_sum: u32,
    cycle_reset_threshold: u32,
    cycles_since_reset: u32,
}

impl AverageCycle {
    fn new(cycle_reset_threshold: u32) -> Self {
        AverageCycle {
            single_cycle_dur: Duration::from_nanos(0),
            dur_sum: 0,
            cycles_sum: 0,
            cycle_reset_threshold,
            cycles_since_reset: cycle_reset_threshold,
        }
    }

    fn insert(&mut self, dur: Duration, cycles: u16) {
        if u32::MAX - self.cycles_sum < cycles as u32 {
            self.cycles_sum = 0;
            self.dur_sum = 0;
            self.cycles_since_reset = 0;
        }
        self.dur_sum += dur.as_nanos() as u64;
        self.cycles_sum += cycles as u32;
        self.cycles_since_reset += cycles as u32;
        if self.single_cycle_dur.is_zero() || self.cycles_since_reset >= self.cycle_reset_threshold
        {
            self.single_cycle_dur = Duration::from_nanos(self.dur_sum / self.cycles_sum as u64);
        }
    }
}

pub struct JitCycleManager {
    arm9_average_cycle: AverageCycle,
    arm7_average_cycle: AverageCycle,
}

impl JitCycleManager {
    pub fn new() -> Self {
        JitCycleManager {
            arm9_average_cycle: AverageCycle::new(100),
            arm7_average_cycle: AverageCycle::new(50),
        }
    }

    pub fn insert(&mut self, cpu_type: CpuType, dur: Duration, cycles: u16) {
        match cpu_type {
            CpuType::ARM9 => self.arm9_average_cycle.insert(dur, cycles),
            CpuType::ARM7 => self.arm7_average_cycle.insert(dur, cycles),
        }

        match cpu_type {
            CpuType::ARM9 => {
                println!(
                    "{:?} average {}ns {} cycles",
                    cpu_type,
                    self.arm9_average_cycle.single_cycle_dur.as_nanos(),
                    self.arm9_average_cycle.cycles_sum,
                );
            }
            CpuType::ARM7 => {
                println!(
                    "{:?} average {}ns {} cycles",
                    cpu_type,
                    self.arm7_average_cycle.single_cycle_dur.as_nanos(),
                    self.arm7_average_cycle.cycles_sum,
                );
            }
        }
    }
}

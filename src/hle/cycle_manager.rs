use crate::hle::CpuType;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

pub trait CycleEvent {
    fn scheduled(&mut self, timestamp: &u64);
    fn trigger(&mut self, delay: u16);
}

pub struct CycleManager {
    cycle_count: [AtomicU64; 2],
    events: [Mutex<VecDeque<(u64, Box<dyn CycleEvent>)>>; 2],
    halted: [AtomicBool; 2],
}

impl CycleManager {
    pub fn new() -> Self {
        CycleManager {
            cycle_count: [AtomicU64::new(0), AtomicU64::new(0)],
            events: [Mutex::new(VecDeque::new()), Mutex::new(VecDeque::new())],
            halted: [AtomicBool::new(false), AtomicBool::new(false)],
        }
    }

    pub fn get_cycle_count<const CPU: CpuType>(&self) -> u64 {
        self.cycle_count[CPU as usize].load(Ordering::Relaxed)
    }

    pub fn add_cycle<const CPU: CpuType>(&self, cycles_to_add: u16) {
        let cycle_count =
            self.cycle_count[CPU as usize].fetch_add(cycles_to_add as u64, Ordering::Relaxed);
        self.check_events_internal::<CPU>(cycle_count);

        if self.halted[!CPU as usize].load(Ordering::Acquire) {
            self.cycle_count[!CPU as usize].fetch_add(cycles_to_add as u64, Ordering::Relaxed);
        }
    }

    pub fn check_events<const CPU: CpuType>(&self) {
        let cycle_count = self.cycle_count[CPU as usize].load(Ordering::Relaxed);
        self.check_events_internal::<CPU>(cycle_count);
    }

    fn check_events_internal<const CPU: CpuType>(&self, cycle_count: u64) {
        let mut triggered_events = Vec::new();
        {
            let mut events = self.events[CPU as usize].lock().unwrap();
            while !events.is_empty() && events.front().unwrap().0 <= cycle_count {
                triggered_events.push(events.pop_front().unwrap());
            }
        }
        for (cycles, mut event) in triggered_events {
            event.trigger((cycle_count - cycles) as u16);
        }
    }

    pub fn schedule<const CPU: CpuType, T: CycleEvent + 'static>(
        &self,
        in_cycles: u32,
        mut event: Box<T>,
    ) -> u64 {
        debug_assert_ne!(in_cycles, 0);
        let cycle_count = self.cycle_count[CPU as usize].load(Ordering::Relaxed);
        let mut events = self.events[CPU as usize].lock().unwrap();
        let event_cycle = cycle_count + in_cycles as u64;
        let index = events
            .binary_search_by_key(&event_cycle, |(cycles, _)| *cycles)
            .unwrap_or_else(|index| index);
        event.scheduled(&event_cycle);
        events.insert(index, (event_cycle, event));
        event_cycle
    }

    pub fn on_halt<const CPU: CpuType>(&self) {
        if self.halted[!CPU as usize].load(Ordering::Acquire) {
            let events_arm9 = self.events[CpuType::ARM9 as usize].lock().unwrap();
            let events_arm7 = self.events[CpuType::ARM7 as usize].lock().unwrap();
            let next_arm9 = events_arm9.front();
            let next_arm7 = events_arm7.front();

            if next_arm9.is_some() || next_arm7.is_some() {
                if next_arm9.is_some()
                    && (next_arm7.is_none() || (next_arm7.unwrap().0 > next_arm9.unwrap().0))
                {
                    self.cycle_count[CpuType::ARM9 as usize]
                        .store(next_arm9.unwrap().0, Ordering::Relaxed);
                } else {
                    self.cycle_count[CpuType::ARM7 as usize]
                        .store(next_arm7.unwrap().0, Ordering::Relaxed);
                }
            }
        }
        self.halted[CPU as usize].store(true, Ordering::Release);
    }

    pub fn on_unhalt<const CPU: CpuType>(&self) {
        self.halted[CPU as usize].store(false, Ordering::Release);
    }
}

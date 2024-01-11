use crate::hle::CpuType;
use crate::utils::FastCell;
use std::sync::atomic::{AtomicU64, Ordering};

pub trait CycleEvent {
    fn scheduled(&mut self, timestamp: &u64);
    fn trigger(&mut self, delay: u16);
}

pub struct CycleManager {
    cycle_count: [AtomicU64; 2],
    events: [FastCell<Vec<(u64, Box<dyn CycleEvent>)>>; 2],
}

impl CycleManager {
    pub fn new() -> Self {
        CycleManager {
            cycle_count: [AtomicU64::new(0), AtomicU64::new(0)],
            events: [FastCell::new(Vec::new()), FastCell::new(Vec::new())],
        }
    }

    pub fn get_cycle_count<const CPU: CpuType>(&self) -> u64 {
        self.cycle_count[CPU as usize].load(Ordering::Relaxed)
    }

    pub fn add_cycle<const CPU: CpuType, const USE_OTHER_COUNTER: bool>(&self, cycles_to_add: u16) {
        let cycle_count = if USE_OTHER_COUNTER {
            self.cycle_count[!CPU as usize].load(Ordering::Relaxed)
        } else {
            self.cycle_count[CPU as usize].fetch_add(cycles_to_add as u64, Ordering::Relaxed)
        };

        let events_triggered = {
            let events = self.events[CPU as usize].borrow();
            if let Some((index, _)) = events
                .iter()
                .rev()
                .enumerate()
                .find(|(_, (cycles, _))| cycle_count < *cycles)
            {
                index
            } else {
                0
            }
        };

        for _ in 0..events_triggered {
            let (cycles, mut event) = self.events[CPU as usize].borrow_mut().pop().unwrap();
            event.trigger((cycle_count - cycles) as u16);
        }
    }

    pub fn schedule<const CPU: CpuType>(
        &self,
        in_cycles: u32,
        mut event: Box<dyn CycleEvent>,
    ) -> u64 {
        debug_assert_ne!(in_cycles, 0);
        let cycle_count = self.cycle_count[CPU as usize].load(Ordering::Relaxed);
        let mut events = self.events[CPU as usize].borrow_mut();
        let event_cycle = cycle_count + in_cycles as u64;
        let index = events
            .binary_search_by(|(cycles, _)| event_cycle.cmp(cycles))
            .unwrap_or_else(|index| index);
        event.scheduled(&event_cycle);
        events.insert(index, (event_cycle, event));
        event_cycle
    }
}

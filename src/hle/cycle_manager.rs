use crate::hle::CpuType;
use std::cell::RefCell;
use std::collections::VecDeque;

pub trait CycleEvent {
    fn scheduled(&mut self, timestamp: &u64);
    fn trigger(&mut self, delay: u16);
}

struct CycleManagerInner {
    cycle_count: [u64; 2],
    events: [VecDeque<(u64, Box<dyn CycleEvent>)>; 2],
}

impl CycleManagerInner {
    fn new() -> Self {
        CycleManagerInner {
            cycle_count: [0; 2],
            events: [VecDeque::new(), VecDeque::new()],
        }
    }
}

pub struct CycleManager {
    inner: RefCell<CycleManagerInner>,
}

impl CycleManager {
    pub fn new() -> Self {
        CycleManager {
            inner: RefCell::new(CycleManagerInner::new()),
        }
    }

    pub fn get_cycle_count<const CPU: CpuType>(&self) -> u64 {
        self.inner.borrow().cycle_count[CPU as usize]
    }

    pub fn add_cycle<const CPU: CpuType>(&self, cycles_to_add: u16) {
        self.inner.borrow_mut().cycle_count[CPU as usize] += cycles_to_add as u64;
        let cycles = self.inner.borrow().cycle_count[CPU as usize];
        self.check_events_internal::<CPU>(cycles);
    }

    pub fn check_events<const CPU: CpuType>(&self) {
        let cycles = self.inner.borrow().cycle_count[CPU as usize];
        self.check_events_internal::<CPU>(cycles);
    }

    fn check_events_internal<const CPU: CpuType>(&self, cycle_count: u64) {
        while {
            let events = &self.inner.borrow().events[CPU as usize];
            !events.is_empty() && events.front().unwrap().0 <= cycle_count
        } {
            let (cycles, mut event) = self.inner.borrow_mut().events[CPU as usize]
                .pop_front()
                .unwrap();
            event.trigger((cycle_count - cycles) as u16);
        }
    }

    pub fn schedule<const CPU: CpuType, T: CycleEvent + 'static>(
        &self,
        in_cycles: u32,
        mut event: Box<T>,
    ) -> u64 {
        debug_assert_ne!(in_cycles, 0);
        let mut inner = self.inner.borrow_mut();
        let cycle_count = inner.cycle_count[CPU as usize];
        let events = &mut inner.events[CPU as usize];
        let event_cycle = cycle_count + in_cycles as u64;
        let index = events
            .binary_search_by_key(&event_cycle, |(cycles, _)| *cycles)
            .unwrap_or_else(|index| index);
        event.scheduled(&event_cycle);
        events.insert(index, (event_cycle, event));
        event_cycle
    }

    pub fn skip_to_next_event(&self) {
        let mut inner = self.inner.borrow_mut();
        let events_arm9 = &inner.events[CpuType::ARM9 as usize];
        let events_arm7 = &inner.events[CpuType::ARM7 as usize];
        let next_arm9 = events_arm9.front();
        let next_arm7 = events_arm7.front();

        if next_arm9.is_some() || next_arm7.is_some() {
            if next_arm9.is_some()
                && (next_arm7.is_none() || (next_arm7.unwrap().0 > next_arm9.unwrap().0))
            {
                inner.cycle_count[CpuType::ARM9 as usize] = next_arm9.unwrap().0;
            } else {
                inner.cycle_count[CpuType::ARM7 as usize] = next_arm7.unwrap().0;
            }
        }
    }
}

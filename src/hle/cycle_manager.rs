use std::cell::RefCell;
use std::collections::VecDeque;
use std::intrinsics::unlikely;

pub trait CycleEvent {
    fn scheduled(&mut self, timestamp: &u64);
    fn trigger(&mut self, delay: u16);
}

struct CycleManagerInner {
    cycle_count: u64,
    events: VecDeque<(u64, Box<dyn CycleEvent>)>,
}

impl CycleManagerInner {
    fn new() -> Self {
        CycleManagerInner {
            cycle_count: 0,
            events: VecDeque::new(),
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

    pub fn get_cycle_count(&self) -> u64 {
        self.inner.borrow().cycle_count
    }

    pub fn add_cycle(&self, cycles_to_add: u16) {
        self.inner.borrow_mut().cycle_count += cycles_to_add as u64;
    }

    #[inline]
    pub fn check_events(&self) {
        let cycle_count = self.inner.borrow().cycle_count;
        while {
            match self.inner.borrow().events.front() {
                None => false,
                Some((cycles, _)) => unlikely(*cycles <= cycle_count),
            }
        } {
            let (cycles, mut event) = self.inner.borrow_mut().events.pop_front().unwrap();
            event.trigger((cycle_count - cycles) as u16);
        }
    }

    pub fn schedule(&self, in_cycles: u32, mut event: Box<dyn CycleEvent>) -> u64 {
        debug_assert_ne!(in_cycles, 0);
        let mut inner = self.inner.borrow_mut();
        let cycle_count = inner.cycle_count;
        let events = &mut inner.events;
        let event_cycle = cycle_count + in_cycles as u64;
        let index = events
            .binary_search_by_key(&event_cycle, |(cycles, _)| *cycles)
            .unwrap_or_else(|index| index);
        event.scheduled(&event_cycle);
        events.insert(index, (event_cycle, event));
        event_cycle
    }

    pub fn jump_to_next_event(&self) {
        let mut inner = self.inner.borrow_mut();
        let events = &inner.events;
        inner.cycle_count = events.front().unwrap().0;
    }
}

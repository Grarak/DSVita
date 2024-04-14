use crate::emu::emu::Emu;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::intrinsics::unlikely;

pub trait CycleEvent {
    fn scheduled(&mut self, timestamp: &u64);
    fn trigger(&mut self, emu: &mut Emu);
}

pub struct CycleManager {
    pub cycle_count: u64,
    events: UnsafeCell<VecDeque<(u64, Box<dyn CycleEvent>)>>,
}

impl CycleManager {
    pub fn new() -> Self {
        CycleManager {
            cycle_count: 0,
            events: UnsafeCell::new(VecDeque::new()),
        }
    }

    pub fn add_cycle(&mut self, cycles_to_add: u16) {
        self.cycle_count += cycles_to_add as u64;
    }

    pub fn check_events(&self, emu: &mut Emu) {
        let cycle_count = self.cycle_count;
        let events = unsafe { self.events.get().as_mut().unwrap_unchecked() };
        while {
            let (cycles, _) = unsafe { events.front().unwrap_unchecked() };
            unlikely(*cycles <= cycle_count)
        } {
            let (_, mut event) = unsafe { events.pop_front().unwrap_unchecked() };
            event.trigger(emu);
        }
    }

    pub fn schedule(&self, in_cycles: u32, mut event: Box<dyn CycleEvent>) -> u64 {
        debug_assert_ne!(in_cycles, 0);
        let event_cycle = self.cycle_count + in_cycles as u64;
        let events = unsafe { self.events.get().as_mut().unwrap() };
        let index = events
            .binary_search_by_key(&event_cycle, |(cycles, _)| *cycles)
            .unwrap_or_else(|index| index);
        event.scheduled(&event_cycle);
        events.insert(index, (event_cycle, event));
        event_cycle
    }

    pub fn jump_to_next_event(&mut self) {
        self.cycle_count = unsafe { (*self.events.get()).front().unwrap_unchecked().0 };
    }
}

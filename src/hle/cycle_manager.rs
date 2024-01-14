use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils::FastCell;
use crate::DEBUG;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};

pub trait CycleEvent {
    fn scheduled(&mut self, timestamp: &u64);
    fn trigger(&mut self, delay: u16);
}

pub struct CycleManager {
    cycle_count: [AtomicU64; 2],
    events: [FastCell<Vec<(u64, Box<dyn CycleEvent>)>>; 2],
    halt_ack_mutex: Mutex<[bool; 2]>,
    halt_ack_cond_var: Condvar,
}

impl CycleManager {
    pub fn new() -> Self {
        CycleManager {
            cycle_count: [AtomicU64::new(0), AtomicU64::new(0)],
            events: [FastCell::new(Vec::new()), FastCell::new(Vec::new())],
            halt_ack_mutex: Mutex::new([false; 2]),
            halt_ack_cond_var: Condvar::new(),
        }
    }

    pub fn get_cycle_count<const CPU: CpuType>(&self) -> u64 {
        self.cycle_count[CPU as usize].load(Ordering::Relaxed)
    }

    pub fn add_cycle<const CPU: CpuType, const CHECK_EVENTS: bool>(&self, cycles_to_add: u16) {
        debug_println!("{:?} adding {} cycles", CPU, cycles_to_add);
        let cycle_count =
            self.cycle_count[CPU as usize].fetch_add(cycles_to_add as u64, Ordering::Relaxed);
        if CHECK_EVENTS {
            self.check_events_internal::<CPU>(cycle_count);
        }
    }

    pub fn check_events<const CPU: CpuType>(&self) {
        let cycle_count = self.cycle_count[CPU as usize].load(Ordering::Relaxed);
        self.check_events_internal::<CPU>(cycle_count);
    }

    fn check_events_internal<const CPU: CpuType>(&self, cycle_count: u64) {
        let events_triggered = {
            let events = self.events[CPU as usize].borrow();

            if DEBUG {
                debug_println!("{:?} total cycles {}", CPU, cycle_count);
                for (cycles, _) in &*events {
                    debug_println!("{:?} scheduled at {}", CPU, cycles);
                }
            }

            if let Some((index, _)) = events
                .iter()
                .rev()
                .enumerate()
                .find(|(_, (cycles, _))| cycle_count < *cycles)
            {
                index
            } else {
                events.len()
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

    pub fn remove_halt_ack<const CPU: CpuType>(&self) {
        let mut ack = self.halt_ack_mutex.lock().unwrap();
        ack[CPU as usize] = false;
    }

    pub fn halt_resolve<const CPU: CpuType>(&self) {
        let mut ack = self.halt_ack_mutex.lock().unwrap();
        if ack[!CPU as usize] {
            {
                let events_arm9 = self.events[CpuType::ARM9 as usize].borrow();
                let events_arm7 = self.events[CpuType::ARM7 as usize].borrow();
                let next_arm9 = events_arm9.last();
                let next_arm7 = events_arm7.last();

                if next_arm7.is_none() || next_arm7.unwrap().0 > next_arm9.unwrap().0 {
                    self.cycle_count[CpuType::ARM9 as usize]
                        .store(next_arm9.unwrap().0, Ordering::SeqCst);
                } else {
                    self.cycle_count[CpuType::ARM7 as usize]
                        .store(next_arm7.unwrap().0, Ordering::SeqCst);
                }
            }
            self.check_events::<{ CpuType::ARM9 }>();
            self.check_events::<{ CpuType::ARM7 }>();
            self.halt_ack_cond_var.notify_one();
        } else {
            ack[CPU as usize] = true;
            let _guard = self
                .halt_ack_cond_var
                .wait_while(ack, |ack| !ack[!CPU as usize])
                .unwrap();
        }
    }
}

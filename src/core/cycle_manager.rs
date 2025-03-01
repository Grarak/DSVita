use crate::core::cpu_regs::CpuRegs;
use crate::core::emu::Emu;
use crate::core::graphics::gpu::Gpu;
use crate::core::hle::sound_nitro::SoundNitro;
use crate::core::hle::wifi_hle::WifiHle;
use crate::core::memory::cartridge::Cartridge;
use crate::core::memory::dma::Dma;
use crate::core::spu::Spu;
use crate::core::timers::Timers;
use crate::core::CpuType::{ARM7, ARM9};
use crate::linked_list::{LinkedList, LinkedListAllocator, LinkedListEntry};
use bilge::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cmp::max;
use std::intrinsics::unlikely;
use std::{mem, ptr};

#[bitsize(16)]
#[derive(FromBits)]
pub struct EventTypeEntry {
    event_type: u4,
    arg: u12,
}

impl EventTypeEntry {
    fn create(event_type: EventType, arg: u16) -> Self {
        EventTypeEntry::new(u4::new(event_type as u8), u12::new(arg))
    }
}

struct CycleEventEntry {
    event_type_entry: EventTypeEntry,
    cycle_count: u64,
}

impl CycleEventEntry {
    fn new(event_type: EventType, arg: u16, cycle_count: u64) -> Self {
        CycleEventEntry {
            event_type_entry: EventTypeEntry::create(event_type, arg),
            cycle_count,
        }
    }
}

#[derive(Default)]
struct CycleEventsListAllocator(Vec<*mut LinkedListEntry<CycleEventEntry>>);

impl LinkedListAllocator<CycleEventEntry> for CycleEventsListAllocator {
    fn allocate(&mut self, value: CycleEventEntry) -> *mut LinkedListEntry<CycleEventEntry> {
        let entry = if self.0.is_empty() {
            unsafe { System.alloc(Layout::new::<LinkedListEntry<CycleEventEntry>>()) as *mut LinkedListEntry<CycleEventEntry> }
        } else {
            unsafe { self.0.pop().unwrap_unchecked() }
        };
        unsafe {
            (*entry).value = value;
            (*entry).previous = ptr::null_mut();
            (*entry).next = ptr::null_mut();
        }
        entry
    }

    fn deallocate(&mut self, entry: *mut LinkedListEntry<CycleEventEntry>) {
        self.0.push(entry);
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum EventType {
    CpuInterruptArm9 = 0,
    CpuInterruptArm7 = 1,
    GpuScanline256 = 2,
    GpuScanline355 = 3,
    SoundCmdHle = 4,
    SoundAlarmHle = 5,
    CartridgeWordReadArm9 = 6,
    CartridgeWordReadArm7 = 7,
    DmaArm9 = 8,
    DmaArm7 = 9,
    SpuSample = 10,
    TimerArm9 = 11,
    TimerArm7 = 12,
    WifiScanHle = 13,
}

pub struct CycleManager {
    cycle_count: u64,
    events: LinkedList<CycleEventEntry, CycleEventsListAllocator>,
    imm_events: Vec<EventTypeEntry>,
    imm_events_swap: Vec<EventTypeEntry>,
}

impl CycleManager {
    pub fn new() -> Self {
        CycleManager {
            cycle_count: 0,
            events: LinkedList::new(),
            imm_events: Vec::new(),
            imm_events_swap: Vec::new(),
        }
    }

    pub fn add_cycles(&mut self, cycle_count: u16) {
        self.cycle_count += cycle_count as u64;
    }

    pub fn get_cycles(&self) -> u64 {
        self.cycle_count
    }

    pub fn check_events(&mut self, emu: &mut Emu) -> bool {
        static LUT: [fn(&mut CycleManager, &mut Emu, u16); EventType::WifiScanHle as usize + 1] = [
            CpuRegs::on_interrupt_event::<{ ARM9 }>,
            CpuRegs::on_interrupt_event::<{ ARM7 }>,
            Gpu::on_scanline256_event,
            Gpu::on_scanline355_event,
            SoundNitro::on_cmd_event,
            SoundNitro::on_alarm_event,
            Cartridge::on_word_read_event::<{ ARM9 }>,
            Cartridge::on_word_read_event::<{ ARM7 }>,
            Dma::on_event::<{ ARM9 }>,
            Dma::on_event::<{ ARM7 }>,
            Spu::on_sample_event,
            Timers::on_overflow_event::<{ ARM9 }>,
            Timers::on_overflow_event::<{ ARM7 }>,
            WifiHle::on_scan_event,
        ];

        self.imm_events_swap.clear();
        mem::swap(&mut self.imm_events, &mut self.imm_events_swap);
        for i in 0..self.imm_events_swap.len() {
            let event_type_entry = &self.imm_events_swap[i];
            let func = unsafe { LUT.get_unchecked(u8::from(event_type_entry.event_type()) as usize) };
            func(self, emu, u16::from(event_type_entry.arg()));
        }

        let cycle_count = self.cycle_count;
        let mut event_triggered = false;
        while {
            let entry = &LinkedList::<_, CycleEventsListAllocator>::deref(self.events.root).value;
            unlikely(entry.cycle_count <= cycle_count)
        } {
            event_triggered = true;
            let entry = self.events.remove_begin();
            let func = unsafe { LUT.get_unchecked(u8::from(entry.event_type_entry.event_type()) as usize) };
            func(self, emu, u16::from(entry.event_type_entry.arg()));
        }
        event_triggered
    }

    pub fn schedule_imm(&mut self, event_type: EventType, arg: u16) {
        self.imm_events.push(EventTypeEntry::create(event_type, arg))
    }

    pub fn schedule(&mut self, in_cycles: u32, event_type: EventType, arg: u16) {
        let event_cycle = self.cycle_count + max(in_cycles, 1) as u64;

        let mut current_node = self.events.root;
        while !current_node.is_null() {
            let entry = LinkedList::<_, CycleEventsListAllocator>::deref(current_node);
            if entry.value.cycle_count > event_cycle {
                self.events.insert_entry_begin(current_node, CycleEventEntry::new(event_type, arg, event_cycle));
                return;
            }
            current_node = entry.next;
        }
        self.events.insert_end(CycleEventEntry::new(event_type, arg, event_cycle));
    }

    pub fn jump_to_next_event(&mut self) {
        self.cycle_count = LinkedList::<_, CycleEventsListAllocator>::deref(self.events.root).value.cycle_count
    }
}

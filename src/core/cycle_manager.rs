use crate::core::cpu_regs::CpuRegs;
use crate::core::emu::Emu;
use crate::core::graphics::gpu::Gpu;
use crate::core::hle::sound_nitro::SoundNitro;
use crate::core::memory::cartridge::Cartridge;
use crate::core::memory::dma::Dma;
use crate::core::spu::Spu;
use crate::core::timers::Timers;
use crate::core::CpuType::{ARM7, ARM9};
use crate::linked_list::{LinkedList, LinkedListAllocator, LinkedListEntry};
use std::alloc::{GlobalAlloc, Layout, System};
use std::intrinsics::unlikely;
use std::ptr;

struct CycleEventEntry {
    cycle_count: u64,
    event_type: EventType,
}

impl CycleEventEntry {
    fn new(cycle_count: u64, event_type: EventType) -> Self {
        CycleEventEntry { cycle_count, event_type }
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

#[derive(Debug)]
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
    cycle_count: u64,
    events: LinkedList<CycleEventEntry, CycleEventsListAllocator>,
}

impl CycleManager {
    pub fn new() -> Self {
        CycleManager {
            cycle_count: 0,
            events: LinkedList::new(),
        }
    }

    pub fn add_cycles(&mut self, cycle_count: u16) {
        self.cycle_count += cycle_count as u64;
    }

    pub fn get_cycles(&self) -> u64 {
        self.cycle_count
    }

    #[inline(always)]
    pub fn check_events(&mut self, emu: &mut Emu) -> bool {
        let cycle_count = self.cycle_count;
        let mut event_triggered = false;
        while {
            let entry = &LinkedList::<_, CycleEventsListAllocator>::deref(self.events.root).value;
            unlikely(entry.cycle_count <= cycle_count)
        } {
            event_triggered = true;
            let entry = self.events.remove_begin();
            match entry.event_type {
                EventType::CpuInterruptArm9 => CpuRegs::on_interrupt_event::<{ ARM9 }>(emu),
                EventType::CpuInterruptArm7 => CpuRegs::on_interrupt_event::<{ ARM7 }>(emu),
                EventType::GpuScanline256 => Gpu::on_scanline256_event(self, emu),
                EventType::GpuScanline355 => Gpu::on_scanline355_event(self, emu),
                EventType::SoundCmdHle => SoundNitro::on_cmd_event(self, emu),
                EventType::SoundAlarmHle(id) => SoundNitro::on_alarm_event(id, self, emu),
                EventType::CartridgeWordReadArm9 => Cartridge::on_word_read_event::<{ ARM9 }>(emu),
                EventType::CartridgeWordReadArm7 => Cartridge::on_word_read_event::<{ ARM7 }>(emu),
                EventType::DmaArm9(channel) => Dma::on_event::<{ ARM9 }>(channel, emu),
                EventType::DmaArm7(channel) => Dma::on_event::<{ ARM7 }>(channel, emu),
                EventType::SpuSample => Spu::on_sample_event(emu),
                EventType::TimerArm9(channel) => Timers::on_overflow_event::<{ ARM9 }>(entry.cycle_count, channel, emu),
                EventType::TimerArm7(channel) => Timers::on_overflow_event::<{ ARM7 }>(entry.cycle_count, channel, emu),
            }
        }
        event_triggered
    }

    pub fn schedule(&mut self, in_cycles: u32, event_type: EventType) -> u64 {
        debug_assert_ne!(in_cycles, 0);
        let event_cycle = self.cycle_count + in_cycles as u64;

        let mut current_node = self.events.root;
        while !current_node.is_null() {
            let entry = LinkedList::<_, CycleEventsListAllocator>::deref(current_node);
            if entry.value.cycle_count > event_cycle {
                self.events.insert_entry_begin(current_node, CycleEventEntry::new(event_cycle, event_type));
                return event_cycle;
            }
            current_node = entry.next;
        }
        self.events.insert_end(CycleEventEntry::new(event_cycle, event_type));
        event_cycle
    }

    pub fn jump_to_next_event(&mut self) {
        self.cycle_count = LinkedList::<_, CycleEventsListAllocator>::deref(self.events.root).value.cycle_count
    }
}

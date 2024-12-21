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
    arg: u8,
}

impl CycleEventEntry {
    fn new(cycle_count: u64, event_type: EventType, arg: u8) -> Self {
        CycleEventEntry { cycle_count, event_type, arg }
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
#[derive(Debug)]
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

    pub fn check_events(&mut self, emu: &mut Emu) -> bool {
        const LUT: [fn(&mut CycleManager, &mut Emu, u64, u8); EventType::TimerArm7 as usize + 1] = [
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
        ];

        let cycle_count = self.cycle_count;
        let mut event_triggered = false;
        while {
            let entry = &LinkedList::<_, CycleEventsListAllocator>::deref(self.events.root).value;
            unlikely(entry.cycle_count <= cycle_count)
        } {
            event_triggered = true;
            let entry = self.events.remove_begin();
            let func = unsafe { LUT.get_unchecked(entry.event_type as usize) };
            func(self, emu, entry.cycle_count, entry.arg);
        }
        event_triggered
    }

    pub fn schedule(&mut self, in_cycles: u32, event_type: EventType) -> u64 {
        self.schedule_with_arg(in_cycles, event_type, 0)
    }

    pub fn schedule_with_arg(&mut self, in_cycles: u32, event_type: EventType, arg: u8) -> u64 {
        debug_assert_ne!(in_cycles, 0);
        let event_cycle = self.cycle_count + in_cycles as u64;

        let mut current_node = self.events.root;
        while !current_node.is_null() {
            let entry = LinkedList::<_, CycleEventsListAllocator>::deref(current_node);
            if entry.value.cycle_count > event_cycle {
                self.events.insert_entry_begin(current_node, CycleEventEntry::new(event_cycle, event_type, arg));
                return event_cycle;
            }
            current_node = entry.next;
        }
        self.events.insert_end(CycleEventEntry::new(event_cycle, event_type, arg));
        event_cycle
    }

    pub fn jump_to_next_event(&mut self) {
        self.cycle_count = LinkedList::<_, CycleEventsListAllocator>::deref(self.events.root).value.cycle_count
    }
}

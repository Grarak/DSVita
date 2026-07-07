use crate::core::cp15::Cp15;
use crate::core::cpu_regs::CpuRegs;
use crate::core::cycle_manager::CycleManager;
use crate::core::div_sqrt::DivSqrt;
use crate::core::graphics::gpu::Gpu;
use crate::core::hle::arm7_hle::Arm7Hle;
use crate::core::input::Input;
use crate::core::ipc::Ipc;
use crate::core::memory::cartridge::Cartridge;
use crate::core::memory::dma::Dma;
use crate::core::memory::mem::Memory;
use crate::core::rtc::Rtc;
use crate::core::spi::{MicSampler, Spi};
use crate::core::spu::{SoundSampler, Spu};
use crate::core::thread_regs::ThreadRegs;
use crate::core::timers::Timers;
use crate::core::wifi::Wifi;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::jit_memory::JitMemory;
use crate::savestate::{Savestate, SavestateContext};
use crate::settings::{Settings, DEFAULT_SETTINGS};
use bilge::prelude::*;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU16, AtomicU32};
use std::sync::{Arc, Mutex};

#[bitsize(32)]
#[derive(Copy, Clone, DebugBits, FromBits)]
pub struct NitroSdkVersion {
    relstep: u16,
    minor: u8,
    major: u8,
}

impl NitroSdkVersion {
    pub fn is_valid(self) -> bool {
        u32::from(self) != u32::MAX
    }

    pub fn rely_on_fs_invalidation(self) -> bool {
        self.is_valid() && self.major() < 5
    }

    pub fn is_twl_sdk(self) -> bool {
        self.is_valid() && self.major() >= 5
    }
}

impl Default for NitroSdkVersion {
    fn default() -> Self {
        NitroSdkVersion::from(u32::MAX)
    }
}

crate::savestate::impl_savestate_bytes!(NitroSdkVersion);

pub struct Emu {
    pub ipc: Ipc,
    pub cartridge: Cartridge,
    pub gpu: Gpu,
    pub cm: CycleManager,
    pub cpu: [CpuRegs; 2],
    pub cp15: Cp15,
    pub input: Input,
    pub mem: Memory,
    pub hle: Arm7Hle,
    pub div_sqrt: DivSqrt,
    pub spi: Spi,
    pub rtc: Rtc,
    pub spu: Spu,
    pub dma: [Dma; 2],
    pub timers: [Timers; 2],
    pub wifi: Wifi,
    pub jit: JitMemory,
    pub settings: Settings,
    pub nitro_sdk_version: NitroSdkVersion,
    pub os_irq_table_addr: u32,
    pub os_irq_handler_thread_switch_addr: u32,
    pub fs_clear_overlay_image_addr: u32,
    pub breakout_imm: bool,
    initialized: bool,
}

impl Emu {
    pub fn new(fps: Arc<AtomicU16>, key_map: Arc<AtomicU32>, touch_points: Arc<AtomicU16>, mic_sampler: Arc<Mutex<MicSampler>>, sound_sampler: NonNull<SoundSampler>, jit: JitMemory) -> Self {
        Emu {
            ipc: Ipc::new(),
            cartridge: Cartridge::new(),
            gpu: Gpu::new(fps),
            cm: CycleManager::new(),
            cpu: [CpuRegs::new(), CpuRegs::new()],
            cp15: Cp15::new(),
            input: Input::new(key_map),
            mem: Memory::new(),
            hle: Arm7Hle::new(),
            div_sqrt: DivSqrt::new(),
            spi: Spi::new(touch_points, mic_sampler),
            rtc: Rtc::new(),
            spu: Spu::new(sound_sampler),
            dma: [Dma::new(), Dma::new()],
            timers: [Timers::new(), Timers::new()],
            wifi: Wifi::new(),
            jit,
            settings: DEFAULT_SETTINGS.clone(),
            nitro_sdk_version: NitroSdkVersion::default(),
            os_irq_table_addr: 0,
            os_irq_handler_thread_switch_addr: 0,
            fs_clear_overlay_image_addr: 0,
            breakout_imm: false,
            initialized: true,
        }
    }

    pub fn reset(&mut self) {
        self.jit.init(&self.settings);
        self.ipc.init(&self.settings);
        self.spi.init(&self.settings);
        if !self.initialized {
            *ARM9.thread_regs() = ThreadRegs::default();
            *ARM7.thread_regs() = ThreadRegs::default();
            self.gpu.init();
            self.cm.init();
            self.cpu = [CpuRegs::new(), CpuRegs::new()];
            self.cp15 = Cp15::new();
            self.mem.init();
            self.hle = Arm7Hle::new();
            self.div_sqrt = DivSqrt::new();
            self.rtc = Rtc::new();
            self.spu.init();
            self.dma = [Dma::new(), Dma::new()];
            self.timers = [Timers::new(), Timers::new()];
            self.wifi = Wifi::new();
        }
        self.nitro_sdk_version = NitroSdkVersion::default();
        self.os_irq_table_addr = 0;
        self.os_irq_handler_thread_switch_addr = 0;
        self.fs_clear_overlay_image_addr = 0;
        self.initialized = false;
    }

    // Guest state only: jit, settings and host resources are absent by construction.
    // breakout_imm/initialized are runtime control flow, not guest state.
    fn savestate(&mut self, state: &mut SavestateContext) {
        self.ipc.savestate(state);
        self.cartridge.savestate(state);
        self.gpu.savestate(state);
        self.cm.savestate(state);
        self.cpu.savestate(state);
        self.cp15.savestate(state);
        self.input.savestate(state);
        self.mem.savestate(state);
        self.hle.savestate(state);
        self.div_sqrt.savestate(state);
        self.spi.savestate(state);
        self.rtc.savestate(state);
        self.spu.savestate(state);
        self.dma.savestate(state);
        self.timers.savestate(state);
        self.wifi.savestate(state);
        self.nitro_sdk_version.savestate(state);
        self.os_irq_table_addr.savestate(state);
        self.os_irq_handler_thread_switch_addr.savestate(state);
        self.fs_clear_overlay_image_addr.savestate(state);
        ARM9.thread_regs().savestate(state);
        ARM7.thread_regs().savestate(state);
    }

    pub fn save_state(&mut self, screenshot: &[u8]) -> Option<Vec<u8>> {
        // Progress split: serialize is a fast field walk, compression dominates
        crate::savestate::op_report(crate::savestate::OpPhase::Serialize, 0);
        let mut state = SavestateContext::new_save();
        self.savestate(&mut state);
        let raw = state.into_data()?;
        Some(crate::savestate::encode_savestate_file(self.settings.arm7_emu() as u8, screenshot, &raw, |done, total| {
            crate::savestate::op_report(crate::savestate::OpPhase::Compress, (5 + done * 90 / total.max(1)) as u8);
        }))
    }

    // Reports load progress/result for the pause-menu dialog; reports are dropped
    // when no dialog armed the op (boot -s loads)
    pub fn load_state(&mut self, data: Vec<u8>) -> bool {
        crate::savestate::op_report(crate::savestate::OpPhase::Decompress, 10);
        let Some(raw) = crate::savestate::decode_savestate_file(&data, self.settings.arm7_emu() as u8) else {
            crate::savestate::op_fail();
            return false;
        };
        crate::savestate::op_report(crate::savestate::OpPhase::Apply, 60);
        let mut state = SavestateContext::new_load(raw);
        self.savestate(&mut state);
        if !state.is_load_successful() {
            crate::savestate::op_fail();
            return false;
        }
        crate::savestate::op_report(crate::savestate::OpPhase::Apply, 85);
        self.savestate_post_load();
        crate::savestate::op_finish_load(data.len());
        true
    }

    pub fn savestate_to_file(&mut self, screenshot: &[u8]) {
        let rom_path = self.cartridge.io.file_path.clone();
        let dir = rom_path.parent().unwrap_or(std::path::Path::new(".")).join("savestates");
        if let Err(err) = std::fs::create_dir_all(&dir) {
            crate::logging::info_println!("Failed to create savestate dir {dir:?}: {err}");
            crate::savestate::op_fail();
            return;
        }
        let stem = rom_path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let mut num = 1u32;
        let mut path = dir.join(format!("{stem}-{num}.sav"));
        while path.exists() {
            num += 1;
            path = dir.join(format!("{stem}-{num}.sav"));
        }
        match self.save_state(screenshot) {
            Some(data) => {
                crate::savestate::op_report(crate::savestate::OpPhase::Write, 95);
                match std::fs::write(&path, &data) {
                    Ok(()) => {
                        crate::logging::info_println!("Savestate ({} bytes) written to {path:?}", data.len());
                        crate::savestate::op_finish_save(data.len());
                    }
                    Err(err) => {
                        crate::logging::info_println!("Failed to write savestate {path:?}: {err}");
                        crate::savestate::op_fail();
                    }
                }
            }
            None => {
                crate::logging::info_println!("Savestate serialization failed");
                crate::savestate::op_fail();
            }
        }
    }

    fn savestate_post_load(&mut self) {
        // Host mappings derive from the restored cp15/wram/vram state
        self.mmu_update_all::<{ ARM9 }>();
        self.mmu_update_all::<{ ARM7 }>();

        // Every compiled block may mismatch the restored memory; full jit reset.
        // (invalidate_blocks only covers itcm/main/vram live ranges — wram has none)
        self.jit.init(&self.settings);

        // Rebase spu channel host pointers onto the restored guest addresses (needs the mmu)
        self.spu_savestate_post_load();
    }
}

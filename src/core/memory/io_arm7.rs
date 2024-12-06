use crate::core::emu::Emu;
use crate::core::memory::dma::Dma;
use crate::core::memory::io_arm7_lut::{IoArm7ReadLut, IoArm7ReadLutUpper, IoArm7ReadLutWifi, IoArm7WriteLut, IoArm7WriteLutWifi};
use crate::core::rtc::Rtc;
use crate::core::spi::Spi;
use crate::core::spu::{SoundSampler, Spu};
use crate::core::timers::Timers;
use crate::core::wifi::Wifi;
use crate::core::CpuType::ARM7;
use crate::utils::Convert;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;

pub struct IoArm7 {
    pub spi: Spi,
    pub rtc: Rtc,
    pub spu: Spu,
    pub dma: Dma,
    pub timers: Timers,
    pub wifi: Wifi,
}

impl IoArm7 {
    pub fn new(touch_points: Arc<AtomicU16>, sound_sampler: Arc<SoundSampler>) -> Self {
        IoArm7 {
            spi: Spi::new(touch_points),
            rtc: Rtc::new(),
            spu: Spu::new(sound_sampler),
            dma: Dma::new(ARM7),
            timers: Timers::new(ARM7),
            wifi: Wifi::new(),
        }
    }

    #[inline(never)]
    pub fn read<T: Convert>(&mut self, addr_offset: u32, emu: &mut Emu) -> T {
        match addr_offset & 0xF00000 {
            0x0 if IoArm7ReadLut::is_in_range(addr_offset) => T::from(IoArm7ReadLut::read(addr_offset, size_of::<T>() as u8, emu)),
            0x100000 if IoArm7ReadLutUpper::is_in_range(addr_offset) => T::from(IoArm7ReadLutUpper::read(addr_offset, size_of::<T>() as u8, emu)),
            0x800000 if IoArm7ReadLutWifi::is_in_range(addr_offset) => T::from(IoArm7ReadLutWifi::read(addr_offset, size_of::<T>() as u8, emu)),
            _ => T::from(0),
        }
    }

    #[inline(never)]
    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T, emu: &mut Emu) {
        match addr_offset & 0xF00000 {
            0x0 if IoArm7WriteLut::is_in_range(addr_offset) => IoArm7WriteLut::write(value.into(), addr_offset, size_of::<T>() as u8, emu),
            0x800000 if IoArm7WriteLutWifi::is_in_range(addr_offset) => IoArm7WriteLutWifi::write(value.into(), addr_offset, size_of::<T>() as u8, emu),
            _ => {}
        }
    }
}

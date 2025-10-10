use crate::core::emu::Emu;
use crate::core::graphics::gpu::DispStat;
use crate::core::ipc::{IpcFifoCnt, IpcSyncCnt};
use crate::core::memory::cartridge::{AuxSpiCnt, RomCtrl};
use crate::core::memory::io_arm7_lut::Arm7Io;
use crate::core::memory::io_arm9_lut::Arm9Io;
use crate::core::timers::TimerCntH;
use crate::core::CpuType::{self, ARM7, ARM9};
use crate::utils::{self, Convert, HeapMemU8};
use std::{mem, ptr};

#[derive(Default)]
pub struct Io {
    mem_arm9: HeapMemU8<{ utils::align_up(size_of::<Arm9Io::Memory>(), 0x100) }>,
    mem_arm7: HeapMemU8<{ utils::align_up(size_of::<Arm7Io::Memory>(), 0x100) }>,
}

impl Emu {
    pub fn io_arm9_read<T: Convert>(&mut self, addr_offset: u32) -> T {
        T::from(0)
    }

    pub fn io_arm9_write<T: Convert>(&mut self, addr_offset: u32, value: T) {}

    pub fn io_arm9_write_fixed_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {}

    pub fn io_arm7_read<T: Convert>(&mut self, addr_offset: u32) -> T {
        T::from(0)
    }

    pub fn io_arm7_write<T: Convert>(&mut self, addr_offset: u32, value: T) {}

    pub fn io_arm7_write_fixed_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {}
}

impl Io {
    pub fn arm9(&mut self) -> &mut Arm9Io::Memory {
        unsafe { mem::transmute(self.mem_arm9.as_mut_ptr()) }
    }

    pub fn arm7(&mut self) -> &mut Arm7Io::Memory {
        unsafe { mem::transmute(self.mem_arm7.as_mut_ptr()) }
    }

    pub fn gpu_disp_stat(&mut self, cpu: CpuType) -> &mut DispStat {
        match cpu {
            ARM9 => &mut self.arm9().gpu_disp_stat,
            ARM7 => &mut self.arm7().gpu_disp_stat,
        }
    }

    pub fn dma_sad(&mut self, cpu: CpuType, channel_num: usize) -> &mut u32 {
        let ptr = match cpu {
            ARM9 => ptr::addr_of_mut!(self.arm9().dma_sad_0),
            ARM7 => ptr::addr_of_mut!(self.arm7().dma_sad_0),
        };
        unsafe { ptr.add(channel_num * 3).as_mut_unchecked() }
    }

    pub fn dma_dad(&mut self, cpu: CpuType, channel_num: usize) -> &mut u32 {
        let ptr = match cpu {
            ARM9 => ptr::addr_of_mut!(self.arm9().dma_dad_0),
            ARM7 => ptr::addr_of_mut!(self.arm7().dma_dad_0),
        };
        unsafe { ptr.add(channel_num * 3).as_mut_unchecked() }
    }

    pub fn dma_cnt(&mut self, cpu: CpuType, channel_num: usize) -> &mut u32 {
        let ptr = match cpu {
            ARM9 => ptr::addr_of_mut!(self.arm9().dma_cnt_0),
            ARM7 => ptr::addr_of_mut!(self.arm7().dma_cnt_0),
        };
        unsafe { ptr.add(channel_num * 3).as_mut_unchecked() }
    }

    pub fn timers_cnt_l(&mut self, cpu: CpuType, channel_num: usize) -> &mut u16 {
        let ptr = match cpu {
            ARM9 => ptr::addr_of_mut!(self.arm9().timers_cnt_l_0),
            ARM7 => ptr::addr_of_mut!(self.arm7().timers_cnt_l_0),
        };
        unsafe { ptr.add(channel_num * 2).as_mut_unchecked() }
    }

    pub fn timers_cnt_h(&mut self, cpu: CpuType, channel_num: usize) -> &mut TimerCntH {
        let ptr = match cpu {
            ARM9 => ptr::addr_of_mut!(self.arm9().timers_cnt_h_0),
            ARM7 => ptr::addr_of_mut!(self.arm7().timers_cnt_h_0),
        };
        unsafe { ptr.add(channel_num * 4).as_mut_unchecked() }
    }

    pub fn ipc_sync_reg(&mut self, cpu: CpuType) -> &mut IpcSyncCnt {
        match cpu {
            ARM9 => &mut self.arm9().ipc_sync_reg,
            ARM7 => &mut self.arm7().ipc_sync_reg,
        }
    }

    pub fn ipc_fifo_cnt(&mut self, cpu: CpuType) -> &mut IpcFifoCnt {
        match cpu {
            ARM9 => &mut self.arm9().ipc_fifo_cnt,
            ARM7 => &mut self.arm7().ipc_fifo_cnt,
        }
    }

    pub fn ipc_fifo_send(&mut self, cpu: CpuType) -> &mut u32 {
        match cpu {
            ARM9 => &mut self.arm9().ipc_fifo_send,
            ARM7 => &mut self.arm7().ipc_fifo_send,
        }
    }

    pub fn cartridge_aux_spi_cnt(&mut self, cpu: CpuType) -> &mut AuxSpiCnt {
        match cpu {
            ARM9 => &mut self.arm9().cartridge_aux_spi_cnt,
            ARM7 => &mut self.arm7().cartridge_aux_spi_cnt,
        }
    }

    pub fn cartridge_aux_spi_data(&mut self, cpu: CpuType) -> &mut u8 {
        match cpu {
            ARM9 => &mut self.arm9().cartridge_aux_spi_data,
            ARM7 => &mut self.arm7().cartridge_aux_spi_data,
        }
    }

    pub fn cartridge_rom_ctrl(&mut self, cpu: CpuType) -> &mut RomCtrl {
        match cpu {
            ARM9 => &mut self.arm9().cartridge_rom_ctrl,
            ARM7 => &mut self.arm7().cartridge_rom_ctrl,
        }
    }

    pub fn cartridge_bus_cmd_out_l(&mut self, cpu: CpuType) -> &mut u32 {
        match cpu {
            ARM9 => &mut self.arm9().cartridge_bus_cmd_out_l,
            ARM7 => &mut self.arm7().cartridge_bus_cmd_out_l,
        }
    }

    pub fn cartridge_bus_cmd_out_h(&mut self, cpu: CpuType) -> &mut u32 {
        match cpu {
            ARM9 => &mut self.arm9().cartridge_bus_cmd_out_h,
            ARM7 => &mut self.arm7().cartridge_bus_cmd_out_h,
        }
    }

    pub fn cpu_ime(&mut self, cpu: CpuType) -> &mut u8 {
        match cpu {
            ARM9 => &mut self.arm9().cpu_ime,
            ARM7 => &mut self.arm7().cpu_ime,
        }
    }

    pub fn cpu_ie(&mut self, cpu: CpuType) -> &mut u32 {
        match cpu {
            ARM9 => &mut self.arm9().cpu_ie,
            ARM7 => &mut self.arm7().cpu_ie,
        }
    }

    pub fn cpu_irf(&mut self, cpu: CpuType) -> &mut u32 {
        match cpu {
            ARM9 => &mut self.arm9().cpu_irf,
            ARM7 => &mut self.arm7().cpu_irf,
        }
    }

    pub fn cpu_post_flg(&mut self, cpu: CpuType) -> &mut u8 {
        match cpu {
            ARM9 => &mut self.arm9().cpu_post_flg,
            ARM7 => &mut self.arm7().cpu_post_flg,
        }
    }
}

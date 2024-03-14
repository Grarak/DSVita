use crate::hle::hle::{get_cm, get_cpu_regs, get_cpu_regs_mut, Hle};
use crate::hle::memory::dma::Dma;
use crate::hle::rtc::Rtc;
use crate::hle::spi::Spi;
use crate::hle::spu::Spu;
use crate::hle::timers::Timers;
use crate::hle::CpuType::ARM7;
use crate::logging::debug_println;
use crate::utils::Convert;
use dspsv_macros::{io_ports_read, io_ports_write};
use std::mem;

pub struct IoArm7 {
    spi: Spi,
    rtc: Rtc,
    spu: Spu,
    pub dma: Dma,
    pub timers: Timers,
}

impl IoArm7 {
    pub fn new() -> Self {
        IoArm7 {
            spi: Spi::new(),
            rtc: Rtc::new(),
            spu: Spu::new(),
            dma: Dma::new(ARM7),
            timers: Timers::new(ARM7),
        }
    }

    pub fn read<T: Convert>(&mut self, addr_offset: u32, hle: &mut Hle) -> T {
        /*
         * Use moving windows to handle reads and writes
         * |0|0|0|  x  |   x   |   x   |   x   |0|0|0|
         *         addr   + 1     + 2     + 3
         */
        let mut bytes_window = [0u8; 10];

        let mut addr_offset_tmp = addr_offset;
        let mut index = 3usize;
        let hle_ptr = hle as *mut Hle;
        let common = unsafe { &mut hle_ptr.as_mut().unwrap_unchecked().common };
        while (index - 3) < mem::size_of::<T>() {
            io_ports_read!(match addr_offset + (index - 3) as u32 {
                io16(0x4) => common.gpu.get_disp_stat::<{ ARM7 }>(),
                io16(0x6) => common.gpu.v_count,
                io32(0xB0) => self.dma.get_sad::<0>(),
                io32(0xB4) => self.dma.get_dad::<0>(),
                io32(0xB8) => self.dma.get_cnt::<0>(),
                io32(0xBC) => self.dma.get_sad::<1>(),
                io32(0xC0) => self.dma.get_dad::<1>(),
                io32(0xC4) => self.dma.get_cnt::<1>(),
                io32(0xC8) => self.dma.get_sad::<2>(),
                io32(0xCC) => self.dma.get_dad::<2>(),
                io32(0xD0) => self.dma.get_cnt::<2>(),
                io32(0xD4) => self.dma.get_sad::<3>(),
                io32(0xD8) => self.dma.get_dad::<3>(),
                io32(0xDC) => self.dma.get_cnt::<3>(),
                io16(0x100) => self.timers.get_cnt_l::<0>(get_cm!(hle)),
                io16(0x102) => self.timers.get_cnt_h::<0>(),
                io16(0x104) => self.timers.get_cnt_l::<1>(get_cm!(hle)),
                io16(0x106) => self.timers.get_cnt_h::<1>(),
                io16(0x108) => self.timers.get_cnt_l::<2>(get_cm!(hle)),
                io16(0x10A) => self.timers.get_cnt_h::<2>(),
                io16(0x10C) => self.timers.get_cnt_l::<3>(get_cm!(hle)),
                io16(0x10E) => self.timers.get_cnt_h::<3>(),
                io16(0x130) => common.input.read().unwrap().key_input,
                io16(0x136) => common.input.read().unwrap().ext_key_in,
                io8(0x138) => self.rtc.get_rtc(),
                io16(0x180) => common.ipc.get_sync_reg::<{ ARM7 }>(),
                io16(0x184) => common.ipc.get_fifo_cnt::<{ ARM7 }>(),
                io16(0x1A0) => common.cartridge.get_aux_spi_cnt::<{ ARM7 }>(),
                io8(0x1A2) => common.cartridge.get_aux_spi_data::<{ ARM7 }>(),
                io32(0x1A4) => common.cartridge.get_rom_ctrl::<{ ARM7 }>(),
                io16(0x1C0) => self.spi.cnt,
                io8(0x1C2) => self.spi.data,
                io8(0x208) => get_cpu_regs!(hle, ARM7).ime,
                io32(0x210) => get_cpu_regs!(hle, ARM7).ie,
                io32(0x214) => get_cpu_regs!(hle, ARM7).irf,
                io8(0x240) => hle.mem.vram.stat,
                io8(0x241) => hle.mem.wram.cnt,
                io8(0x300) => get_cpu_regs!(hle, ARM7).post_flg,
                io8(0x301) => get_cpu_regs!(hle, ARM7).halt_cnt,
                io32(0x400) => self.spu.get_cnt(0),
                io32(0x410) => self.spu.get_cnt(1),
                io32(0x420) => self.spu.get_cnt(2),
                io32(0x430) => self.spu.get_cnt(3),
                io32(0x440) => self.spu.get_cnt(4),
                io32(0x450) => self.spu.get_cnt(5),
                io32(0x460) => self.spu.get_cnt(6),
                io32(0x470) => self.spu.get_cnt(7),
                io32(0x480) => self.spu.get_cnt(8),
                io32(0x490) => self.spu.get_cnt(9),
                io32(0x4A0) => self.spu.get_cnt(10),
                io32(0x4B0) => self.spu.get_cnt(11),
                io32(0x4C0) => self.spu.get_cnt(12),
                io32(0x4D0) => self.spu.get_cnt(13),
                io32(0x4E0) => self.spu.get_cnt(14),
                io32(0x4F0) => self.spu.get_cnt(15),
                io16(0x500) => self.spu.main_sound_cnt,
                io16(0x504) => todo!(),
                io8(0x508) => self.spu.get_snd_cap_cnt(0),
                io8(0x509) => self.spu.get_snd_cap_cnt(1),
                io32(0x510) => todo!(),
                io32(0x518) => todo!(),
                io32(0x100000) => common.ipc.fifo_recv::<{ ARM7 }>(hle),
                io32(0x100010) => todo!(),
                io16(0x800006) => todo!(),
                io16(0x800010) => todo!(),
                io16(0x800012) => todo!(),
                io16(0x800018) => todo!(),
                io16(0x80001A) => todo!(),
                io16(0x80001C) => todo!(),
                io16(0x800020) => todo!(),
                io16(0x800022) => todo!(),
                io16(0x800024) => todo!(),
                io16(0x80002A) => todo!(),
                io16(0x800030) => todo!(),
                io16(0x80003C) => todo!(),
                io16(0x800040) => todo!(),
                io16(0x800050) => todo!(),
                io16(0x800052) => todo!(),
                io16(0x800054) => todo!(),
                io16(0x800056) => todo!(),
                io16(0x800058) => todo!(),
                io16(0x80005A) => todo!(),
                io16(0x80005C) => todo!(),
                io16(0x800060) => todo!(),
                io16(0x800062) => todo!(),
                io16(0x800064) => todo!(),
                io16(0x800068) => todo!(),
                io16(0x80006C) => todo!(),
                io16(0x800074) => todo!(),
                io16(0x800076) => todo!(),
                io16(0x800080) => todo!(),
                io16(0x80008C) => todo!(),
                io16(0x800090) => todo!(),
                io16(0x8000A0) => todo!(),
                io16(0x8000A4) => todo!(),
                io16(0x8000A8) => todo!(),
                io16(0x8000B0) => todo!(),
                io16(0x8000E8) => todo!(),
                io16(0x8000EA) => todo!(),
                io16(0x800110) => todo!(),
                io16(0x80011C) => todo!(),
                io16(0x800120) => todo!(),
                io16(0x800122) => todo!(),
                io16(0x800124) => todo!(),
                io16(0x800128) => todo!(),
                io16(0x800130) => todo!(),
                io16(0x800132) => todo!(),
                io16(0x800134) => todo!(),
                io16(0x800140) => todo!(),
                io16(0x800142) => todo!(),
                io16(0x800144) => todo!(),
                io16(0x800146) => todo!(),
                io16(0x800148) => todo!(),
                io16(0x80014A) => todo!(),
                io16(0x80014C) => todo!(),
                io16(0x800150) => todo!(),
                io16(0x800154) => todo!(),
                io16(0x80015C) => todo!(),
                _ => {
                    if index == 3 {
                        debug_println!("{:?} unknown io port read at {:x}", ARM7, addr_offset);
                    }

                    bytes_window[index] = 0;
                }
            });
            index += 1;
        }
        T::from(u32::from_le_bytes([
            bytes_window[3],
            bytes_window[4],
            bytes_window[5],
            bytes_window[6],
        ]))
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T, hle: &mut Hle) {
        let bytes = value.into().to_le_bytes();
        let bytes = &bytes[..mem::size_of::<T>()];
        /*
         * Use moving windows to handle reads and writes
         * |0|0|0|  x  |   x   |   x   |   x   |0|0|0|
         *         addr   + 1     + 2     + 3
         */
        let mut bytes_window = [0u8; 10];
        let mut mask_window = [0u8; 10];
        bytes_window[3..3 + mem::size_of::<T>()].copy_from_slice(bytes);
        mask_window[3..3 + mem::size_of::<T>()].fill(0xFF);

        let mut addr_offset_tmp = addr_offset;
        let mut index = 3usize;
        let hle_ptr = hle as *mut Hle;
        let common = unsafe { &mut hle_ptr.as_mut().unwrap_unchecked().common };
        while (index - 3) < mem::size_of::<T>() {
            io_ports_write!(match addr_offset + (index - 3) as u32 {
                io16(0x4) => common.gpu.set_disp_stat::<{ ARM7 }>(mask, value),
                io32(0xB0) => todo!(),
                io32(0xB4) => todo!(),
                io32(0xB8) => todo!(),
                io32(0xBC) => todo!(),
                io32(0xC0) => todo!(),
                io32(0xC4) => todo!(),
                io32(0xC8) => todo!(),
                io32(0xCC) => todo!(),
                io32(0xD0) => todo!(),
                io32(0xD4) => self.dma.set_sad::<3>(mask, value),
                io32(0xD8) => self.dma.set_dad::<3>(mask, value),
                io32(0xDC) => self.dma.set_cnt::<3>(mask, value, hle),
                io16(0x100) => self.timers.set_cnt_l::<0>(mask, value),
                io16(0x102) => self.timers.set_cnt_h::<0>(mask, value, hle),
                io16(0x104) => self.timers.set_cnt_l::<1>(mask, value),
                io16(0x106) => self.timers.set_cnt_h::<1>(mask, value, hle),
                io16(0x108) => self.timers.set_cnt_l::<2>(mask, value),
                io16(0x10A) => self.timers.set_cnt_h::<2>(mask, value, hle),
                io16(0x10C) => self.timers.set_cnt_l::<3>(mask, value),
                io16(0x10E) => self.timers.set_cnt_h::<3>(mask, value, hle),
                io8(0x138) => self.rtc.set_rtc(value),
                io16(0x180) => common.ipc.set_sync_reg::<{ ARM7 }>(mask, value, hle),
                io16(0x184) => common.ipc.set_fifo_cnt::<{ ARM7 }>(mask, value, hle),
                io32(0x188) => common.ipc.fifo_send::<{ ARM7 }>(mask, value, hle),
                io16(0x1A0) => common.cartridge.set_aux_spi_cnt::<{ ARM7 }>(mask, value),
                io8(0x1A2) => common.cartridge.set_aux_spi_data::<{ ARM7 }>(value),
                io32(0x1A4) => common.cartridge.set_rom_ctrl::<{ ARM7 }>(mask, value, hle),
                io32(0x1A8) => common.cartridge.set_bus_cmd_out_l::<{ ARM7 }>(mask, value),
                io32(0x1AC) => common.cartridge.set_bus_cmd_out_h::<{ ARM7 }>(mask, value),
                io16(0x1C0) => self.spi.set_cnt(mask, value),
                io8(0x1C2) => self.spi.set_data(value),
                io8(0x208) => get_cpu_regs_mut!(hle, ARM7).set_ime(value, get_cm!(hle)),
                io32(0x210) => get_cpu_regs_mut!(hle, ARM7).set_ie(mask, value, get_cm!(hle)),
                io32(0x214) => get_cpu_regs_mut!(hle, ARM7).set_irf(mask, value),
                io8(0x300) => get_cpu_regs_mut!(hle, ARM7).set_post_flg(value),
                io8(0x301) => todo!(),
                io32(0x400) => self.spu.set_cnt(0, mask, value),
                io32(0x404) => self.spu.set_sad(0, mask, value),
                io16(0x408) => self.spu.set_tmr(0, mask, value),
                io16(0x40A) => self.spu.set_pnt(0, mask, value),
                io32(0x40C) => self.spu.set_len(0, mask, value),
                io32(0x410) => self.spu.set_cnt(1, mask, value),
                io32(0x414) => self.spu.set_sad(1, mask, value),
                io16(0x418) => self.spu.set_tmr(1, mask, value),
                io16(0x41A) => self.spu.set_pnt(1, mask, value),
                io32(0x41C) => self.spu.set_len(1, mask, value),
                io32(0x420) => self.spu.set_cnt(2, mask, value),
                io32(0x424) => self.spu.set_sad(2, mask, value),
                io16(0x428) => self.spu.set_tmr(2, mask, value),
                io16(0x42A) => self.spu.set_pnt(2, mask, value),
                io32(0x42C) => self.spu.set_len(2, mask, value),
                io32(0x430) => self.spu.set_cnt(3, mask, value),
                io32(0x434) => self.spu.set_sad(3, mask, value),
                io16(0x438) => self.spu.set_tmr(3, mask, value),
                io16(0x43A) => self.spu.set_pnt(3, mask, value),
                io32(0x43C) => self.spu.set_len(3, mask, value),
                io32(0x440) => self.spu.set_cnt(4, mask, value),
                io32(0x444) => self.spu.set_sad(4, mask, value),
                io16(0x448) => self.spu.set_tmr(4, mask, value),
                io16(0x44A) => self.spu.set_pnt(4, mask, value),
                io32(0x44C) => self.spu.set_len(4, mask, value),
                io32(0x450) => self.spu.set_cnt(5, mask, value),
                io32(0x454) => self.spu.set_sad(5, mask, value),
                io16(0x458) => self.spu.set_tmr(5, mask, value),
                io16(0x45A) => self.spu.set_pnt(5, mask, value),
                io32(0x45C) => self.spu.set_len(5, mask, value),
                io32(0x460) => self.spu.set_cnt(6, mask, value),
                io32(0x464) => self.spu.set_sad(6, mask, value),
                io16(0x468) => self.spu.set_tmr(6, mask, value),
                io16(0x46A) => self.spu.set_pnt(6, mask, value),
                io32(0x46C) => self.spu.set_len(6, mask, value),
                io32(0x470) => self.spu.set_cnt(7, mask, value),
                io32(0x474) => self.spu.set_sad(7, mask, value),
                io16(0x478) => self.spu.set_tmr(7, mask, value),
                io16(0x47A) => self.spu.set_pnt(7, mask, value),
                io32(0x47C) => self.spu.set_len(7, mask, value),
                io32(0x480) => self.spu.set_cnt(8, mask, value),
                io32(0x484) => self.spu.set_sad(8, mask, value),
                io16(0x488) => self.spu.set_tmr(8, mask, value),
                io16(0x48A) => self.spu.set_pnt(8, mask, value),
                io32(0x48C) => self.spu.set_len(8, mask, value),
                io32(0x490) => self.spu.set_cnt(9, mask, value),
                io32(0x494) => self.spu.set_sad(9, mask, value),
                io16(0x498) => self.spu.set_tmr(9, mask, value),
                io16(0x49A) => self.spu.set_pnt(9, mask, value),
                io32(0x49C) => self.spu.set_len(9, mask, value),
                io32(0x4A0) => self.spu.set_cnt(10, mask, value),
                io32(0x4A4) => self.spu.set_sad(10, mask, value),
                io16(0x4A8) => self.spu.set_tmr(10, mask, value),
                io16(0x4AA) => self.spu.set_pnt(10, mask, value),
                io32(0x4AC) => self.spu.set_len(10, mask, value),
                io32(0x4B0) => self.spu.set_cnt(11, mask, value),
                io32(0x4B4) => self.spu.set_sad(11, mask, value),
                io16(0x4B8) => self.spu.set_tmr(11, mask, value),
                io16(0x4BA) => self.spu.set_pnt(11, mask, value),
                io32(0x4BC) => self.spu.set_len(11, mask, value),
                io32(0x4C0) => self.spu.set_cnt(12, mask, value),
                io32(0x4C4) => self.spu.set_sad(12, mask, value),
                io16(0x4C8) => self.spu.set_tmr(12, mask, value),
                io16(0x4CA) => self.spu.set_pnt(12, mask, value),
                io32(0x4CC) => self.spu.set_len(12, mask, value),
                io32(0x4D0) => self.spu.set_cnt(13, mask, value),
                io32(0x4D4) => self.spu.set_sad(13, mask, value),
                io16(0x4D8) => self.spu.set_tmr(13, mask, value),
                io16(0x4DA) => self.spu.set_pnt(13, mask, value),
                io32(0x4DC) => self.spu.set_len(13, mask, value),
                io32(0x4E0) => self.spu.set_cnt(14, mask, value),
                io32(0x4E4) => self.spu.set_sad(14, mask, value),
                io16(0x4E8) => self.spu.set_tmr(14, mask, value),
                io16(0x4EA) => self.spu.set_pnt(14, mask, value),
                io32(0x4EC) => self.spu.set_len(14, mask, value),
                io32(0x4F0) => self.spu.set_cnt(15, mask, value),
                io32(0x4F4) => self.spu.set_sad(15, mask, value),
                io16(0x4F8) => self.spu.set_tmr(15, mask, value),
                io16(0x4FA) => self.spu.set_pnt(15, mask, value),
                io32(0x4FC) => self.spu.set_len(15, mask, value),
                io16(0x500) => self.spu.set_main_sound_cnt(mask, value),
                io16(0x504) => self.spu.set_sound_bias(mask, value),
                io8(0x508) => self.spu.set_snd_cap_cnt(0, value),
                io8(0x509) => self.spu.set_snd_cap_cnt(1, value),
                io32(0x510) => self.spu.set_snd_cap_dad(0, mask, value),
                io16(0x514) => self.spu.set_snd_cap_len(0, mask, value),
                io32(0x518) => self.spu.set_snd_cap_dad(1, mask, value),
                io16(0x51C) => self.spu.set_snd_cap_len(1, mask, value),
                io16(0x800006) => todo!(),
                io16(0x800010) => todo!(),
                io16(0x800012) => todo!(),
                io16(0x800018) => todo!(),
                io16(0x80001A) => todo!(),
                io16(0x80001C) => todo!(),
                io16(0x800020) => todo!(),
                io16(0x800022) => todo!(),
                io16(0x800024) => todo!(),
                io16(0x80002A) => todo!(),
                io16(0x800030) => todo!(),
                io16(0x80003C) => todo!(),
                io16(0x800040) => todo!(),
                io16(0x800050) => todo!(),
                io16(0x800052) => todo!(),
                io16(0x800056) => todo!(),
                io16(0x800058) => todo!(),
                io16(0x80005A) => todo!(),
                io16(0x80005C) => todo!(),
                io16(0x800062) => todo!(),
                io16(0x800064) => todo!(),
                io16(0x800068) => todo!(),
                io16(0x80006C) => todo!(),
                io16(0x800070) => todo!(),
                io16(0x800074) => todo!(),
                io16(0x800076) => todo!(),
                io16(0x800080) => todo!(),
                io16(0x80008C) => todo!(),
                io16(0x800090) => todo!(),
                io16(0x8000A0) => todo!(),
                io16(0x8000A4) => todo!(),
                io16(0x8000A8) => todo!(),
                io16(0x8000AC) => todo!(),
                io16(0x8000AE) => todo!(),
                io16(0x8000E8) => todo!(),
                io16(0x8000EA) => todo!(),
                io16(0x800110) => todo!(),
                io16(0x80011C) => todo!(),
                io16(0x800120) => todo!(),
                io16(0x800122) => todo!(),
                io16(0x800124) => todo!(),
                io16(0x800128) => todo!(),
                io16(0x800130) => todo!(),
                io16(0x800132) => todo!(),
                io16(0x800134) => todo!(),
                io16(0x800140) => todo!(),
                io16(0x800142) => todo!(),
                io16(0x800144) => todo!(),
                io16(0x800146) => todo!(),
                io16(0x800148) => todo!(),
                io16(0x80014A) => todo!(),
                io16(0x80014C) => todo!(),
                io16(0x800150) => todo!(),
                io16(0x800154) => todo!(),
                io16(0x800158) => todo!(),
                io16(0x80015A) => todo!(),
                io16(0x80021C) => todo!(),
                _ => {
                    if index == 3 {
                        debug_println!(
                            "{:?} unknown io port write at {:x} with value {:x}",
                            ARM7,
                            addr_offset,
                            value.into()
                        );
                    }
                }
            });
            index += 1;
        }
    }
}

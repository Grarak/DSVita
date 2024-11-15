use crate::core::emu::{
    get_cm, get_common, get_common_mut, get_cpu_regs, get_cpu_regs_mut, get_mem, get_spu, get_spu_mut, io_dma, io_dma_mut, io_rtc, io_rtc_mut, io_spi, io_spi_mut, io_timers, io_timers_mut, io_wifi,
    io_wifi_mut,
};
use crate::core::wifi::PaketType;
use crate::core::CpuType::ARM7;
use dsvita_macros::{io_read, io_write};

io_read!(
    IoArm7ReadLut,
    [
        (io16(0x4), |emu| get_common!(emu).gpu.get_disp_stat::<{ ARM7 }>()),
        (io16(0x6), |emu| get_common!(emu).gpu.v_count),
        (io32(0xB0), |emu| io_dma!(emu, ARM7).get_sad::<0>()),
        (io32(0xB4), |emu| io_dma!(emu, ARM7).get_dad::<0>()),
        (io32(0xB8), |emu| io_dma!(emu, ARM7).get_cnt::<0>()),
        (io32(0xBC), |emu| io_dma!(emu, ARM7).get_sad::<1>()),
        (io32(0xC0), |emu| io_dma!(emu, ARM7).get_dad::<1>()),
        (io32(0xC4), |emu| io_dma!(emu, ARM7).get_cnt::<1>()),
        (io32(0xC8), |emu| io_dma!(emu, ARM7).get_sad::<2>()),
        (io32(0xCC), |emu| io_dma!(emu, ARM7).get_dad::<2>()),
        (io32(0xD0), |emu| io_dma!(emu, ARM7).get_cnt::<2>()),
        (io32(0xD4), |emu| io_dma!(emu, ARM7).get_sad::<3>()),
        (io32(0xD8), |emu| io_dma!(emu, ARM7).get_dad::<3>()),
        (io32(0xDC), |emu| io_dma!(emu, ARM7).get_cnt::<3>()),
        (io16(0x100), |emu| io_timers_mut!(emu, ARM7).get_cnt_l::<0>(get_cm!(emu))),
        (io16(0x102), |emu| io_timers!(emu, ARM7).get_cnt_h::<0>()),
        (io16(0x104), |emu| io_timers_mut!(emu, ARM7).get_cnt_l::<1>(get_cm!(emu))),
        (io16(0x106), |emu| io_timers!(emu, ARM7).get_cnt_h::<1>()),
        (io16(0x108), |emu| io_timers_mut!(emu, ARM7).get_cnt_l::<2>(get_cm!(emu))),
        (io16(0x10A), |emu| io_timers!(emu, ARM7).get_cnt_h::<2>()),
        (io16(0x10C), |emu| io_timers_mut!(emu, ARM7).get_cnt_l::<3>(get_cm!(emu))),
        (io16(0x10E), |emu| io_timers!(emu, ARM7).get_cnt_h::<3>()),
        (io16(0x130), |emu| get_common!(emu).input.get_key_input()),
        (io16(0x136), |emu| get_common!(emu).input.get_ext_key_in()),
        (io8(0x138), |emu| io_rtc!(emu).get_rtc()),
        (io16(0x180), |emu| get_common!(emu).ipc.get_sync_reg::<{ ARM7 }>()),
        (io16(0x184), |emu| get_common!(emu).ipc.get_fifo_cnt::<{ ARM7 }>()),
        (io16(0x1A0), |emu| get_common!(emu).cartridge.get_aux_spi_cnt::<{ ARM7 }>()),
        (io8(0x1A2), |emu| get_common!(emu).cartridge.get_aux_spi_data::<{ ARM7 }>()),
        (io32(0x1A4), |emu| get_common!(emu).cartridge.get_rom_ctrl::<{ ARM7 }>()),
        (io16(0x1C0), |emu| io_spi!(emu).cnt),
        (io8(0x1C2), |emu| io_spi!(emu).data),
        (io8(0x208), |emu| get_cpu_regs!(emu, ARM7).ime),
        (io32(0x210), |emu| get_cpu_regs!(emu, ARM7).ie),
        (io32(0x214), |emu| get_cpu_regs!(emu, ARM7).irf),
        (io8(0x240), |emu| get_mem!(emu).vram.stat),
        (io8(0x241), |emu| get_mem!(emu).wram.cnt),
        (io8(0x300), |emu| get_cpu_regs!(emu, ARM7).post_flg),
        (io8(0x301), |emu| get_cpu_regs!(emu, ARM7).halt_cnt),
        (io32(0x400), |emu| get_spu!(emu).get_cnt(0)),
        (io32(0x410), |emu| get_spu!(emu).get_cnt(1)),
        (io32(0x420), |emu| get_spu!(emu).get_cnt(2)),
        (io32(0x430), |emu| get_spu!(emu).get_cnt(3)),
        (io32(0x440), |emu| get_spu!(emu).get_cnt(4)),
        (io32(0x450), |emu| get_spu!(emu).get_cnt(5)),
        (io32(0x460), |emu| get_spu!(emu).get_cnt(6)),
        (io32(0x470), |emu| get_spu!(emu).get_cnt(7)),
        (io32(0x480), |emu| get_spu!(emu).get_cnt(8)),
        (io32(0x490), |emu| get_spu!(emu).get_cnt(9)),
        (io32(0x4A0), |emu| get_spu!(emu).get_cnt(10)),
        (io32(0x4B0), |emu| get_spu!(emu).get_cnt(11)),
        (io32(0x4C0), |emu| get_spu!(emu).get_cnt(12)),
        (io32(0x4D0), |emu| get_spu!(emu).get_cnt(13)),
        (io32(0x4E0), |emu| get_spu!(emu).get_cnt(14)),
        (io32(0x4F0), |emu| get_spu!(emu).get_cnt(15)),
        (io16(0x500), |emu| get_spu!(emu).get_main_sound_cnt()),
        (io16(0x504), |emu| todo!()),
        (io8(0x508), |emu| get_spu!(emu).get_snd_cap_cnt(0)),
        (io8(0x509), |emu| get_spu!(emu).get_snd_cap_cnt(1)),
        (io32(0x510), |emu| todo!()),
        (io32(0x518), |emu| todo!()),
    ]
);

io_read!(
    IoArm7ReadLutUpper,
    [(io32(0x100000), |emu| get_common_mut!(emu).ipc.fifo_recv::<{ ARM7 }>(emu)), (io32(0x100010), |emu| todo!())]
);

io_read!(
    IoArm7ReadLutWifi,
    [
        (io16(0x800006), |emu| io_wifi!(emu).w_mode_wep),
        (io16(0x800008), |emu| io_wifi!(emu).w_txstat_cnt),
        (io16(0x800010), |emu| io_wifi!(emu).w_irf),
        (io16(0x800012), |emu| io_wifi!(emu).w_ie),
        (io16(0x800018), |emu| io_wifi!(emu).w_macaddr[0]),
        (io16(0x80001A), |emu| io_wifi!(emu).w_macaddr[1]),
        (io16(0x80001C), |emu| io_wifi!(emu).w_macaddr[2]),
        (io16(0x800020), |emu| io_wifi!(emu).w_bssid[0]),
        (io16(0x800022), |emu| io_wifi!(emu).w_bssid[1]),
        (io16(0x800024), |emu| io_wifi!(emu).w_bssid[2]),
        (io16(0x80002A), |emu| io_wifi!(emu).w_aid_full),
        (io16(0x800030), |emu| io_wifi!(emu).w_rxcnt),
        (io16(0x80003C), |emu| io_wifi!(emu).w_powerstate),
        (io16(0x800040), |emu| io_wifi!(emu).w_powerforce),
        (io16(0x800050), |emu| io_wifi!(emu).w_rxbuf_begin),
        (io16(0x800052), |emu| io_wifi!(emu).w_rxbuf_end),
        (io16(0x800054), |emu| io_wifi!(emu).w_rxbuf_wrcsr),
        (io16(0x800056), |emu| io_wifi!(emu).w_rxbuf_wr_addr),
        (io16(0x800058), |emu| io_wifi!(emu).w_rxbuf_rd_addr),
        (io16(0x80005A), |emu| io_wifi!(emu).w_rxbuf_readcsr),
        (io16(0x80005C), |emu| io_wifi!(emu).w_rxbuf_count),
        (io16(0x800060), |emu| io_wifi_mut!(emu).get_w_rxbuf_rd_data(emu)),
        (io16(0x800062), |emu| io_wifi!(emu).w_rxbuf_gap),
        (io16(0x800064), |emu| io_wifi!(emu).w_rxbuf_gapdisp),
        (io16(0x800068), |emu| io_wifi!(emu).w_txbuf_wr_addr),
        (io16(0x80006C), |emu| io_wifi!(emu).w_txbuf_count),
        (io16(0x800074), |emu| io_wifi!(emu).w_txbuf_gap),
        (io16(0x800076), |emu| io_wifi!(emu).w_txbuf_gapdisp),
        (io16(0x800080), |emu| io_wifi!(emu).get_w_txbuf_loc(PaketType::BeaconFrame)),
        (io16(0x80008C), |emu| io_wifi!(emu).w_beacon_int),
        (io16(0x800090), |emu| io_wifi!(emu).get_w_txbuf_loc(PaketType::CmdFrame)),
        (io16(0x800094), |emu| io_wifi!(emu).w_txbuf_reply1),
        (io16(0x800098), |emu| io_wifi!(emu).w_txbuf_reply2),
        (io16(0x8000A0), |emu| io_wifi!(emu).get_w_txbuf_loc(PaketType::Loc1Frame)),
        (io16(0x8000A4), |emu| io_wifi!(emu).get_w_txbuf_loc(PaketType::Loc2Frame)),
        (io16(0x8000A8), |emu| io_wifi!(emu).get_w_txbuf_loc(PaketType::Loc3Frame)),
        (io16(0x8000B0), |emu| io_wifi!(emu).w_txreq_read),
        (io16(0x8000B8), |emu| io_wifi!(emu).w_txstat),
        (io16(0x8000E8), |emu| io_wifi!(emu).w_us_countcnt),
        (io16(0x8000EE), |emu| io_wifi!(emu).w_cmd_countcnt),
        (io16(0x8000EA), |emu| io_wifi!(emu).w_us_comparecnt),
        (io16(0x8000F0), |emu| io_wifi!(emu).get_w_us_compare(0)),
        (io16(0x8000F2), |emu| io_wifi!(emu).get_w_us_compare(1)),
        (io16(0x8000F4), |emu| io_wifi!(emu).get_w_us_compare(2)),
        (io16(0x8000F6), |emu| io_wifi!(emu).get_w_us_compare(3)),
        (io16(0x8000F8), |emu| io_wifi!(emu).get_w_us_count(0)),
        (io16(0x8000FA), |emu| io_wifi!(emu).get_w_us_count(1)),
        (io16(0x8000FC), |emu| io_wifi!(emu).get_w_us_count(2)),
        (io16(0x8000FE), |emu| io_wifi!(emu).get_w_us_count(3)),
        (io16(0x800110), |emu| io_wifi!(emu).w_pre_beacon),
        (io16(0x800118), |emu| io_wifi!(emu).w_cmd_count),
        (io16(0x80011C), |emu| io_wifi!(emu).w_beacon_count),
        (io16(0x800120), |emu| io_wifi!(emu).w_config[0]),
        (io16(0x800122), |emu| io_wifi!(emu).w_config[1]),
        (io16(0x800124), |emu| io_wifi!(emu).w_config[2]),
        (io16(0x800128), |emu| io_wifi!(emu).w_config[3]),
        (io16(0x800130), |emu| io_wifi!(emu).w_config[4]),
        (io16(0x800132), |emu| io_wifi!(emu).w_config[5]),
        (io16(0x800134), |emu| io_wifi!(emu).w_post_beacon),
        (io16(0x800140), |emu| io_wifi!(emu).w_config[6]),
        (io16(0x800142), |emu| io_wifi!(emu).w_config[7]),
        (io16(0x800144), |emu| io_wifi!(emu).w_config[8]),
        (io16(0x800146), |emu| io_wifi!(emu).w_config[9]),
        (io16(0x800148), |emu| io_wifi!(emu).w_config[10]),
        (io16(0x80014A), |emu| io_wifi!(emu).w_config[11]),
        (io16(0x80014C), |emu| io_wifi!(emu).w_config[12]),
        (io16(0x800150), |emu| io_wifi!(emu).w_config[13]),
        (io16(0x800154), |emu| io_wifi!(emu).w_config[14]),
        (io16(0x80015C), |emu| io_wifi!(emu).w_bb_read),
        (io16(0x800210), |emu| io_wifi!(emu).w_tx_seqno),
    ]
);

io_write!(
    IoArm7WriteLut,
    [
        (io16(0x4), |mask, value, emu| get_common_mut!(emu).gpu.set_disp_stat::<{ ARM7 }>(mask, value)),
        (io32(0xB0), |mask, value, emu| io_dma_mut!(emu, ARM7).set_sad::<0>(mask, value)),
        (io32(0xB4), |mask, value, emu| io_dma_mut!(emu, ARM7).set_dad::<0>(mask, value)),
        (io32(0xB8), |mask, value, emu| io_dma_mut!(emu, ARM7).set_cnt::<0>(mask, value, emu)),
        (io32(0xBC), |mask, value, emu| io_dma_mut!(emu, ARM7).set_sad::<1>(mask, value)),
        (io32(0xC0), |mask, value, emu| io_dma_mut!(emu, ARM7).set_dad::<1>(mask, value)),
        (io32(0xC4), |mask, value, emu| io_dma_mut!(emu, ARM7).set_cnt::<1>(mask, value, emu)),
        (io32(0xC8), |mask, value, emu| io_dma_mut!(emu, ARM7).set_sad::<2>(mask, value)),
        (io32(0xCC), |mask, value, emu| io_dma_mut!(emu, ARM7).set_dad::<2>(mask, value)),
        (io32(0xD0), |mask, value, emu| io_dma_mut!(emu, ARM7).set_cnt::<2>(mask, value, emu)),
        (io32(0xD4), |mask, value, emu| io_dma_mut!(emu, ARM7).set_sad::<3>(mask, value)),
        (io32(0xD8), |mask, value, emu| io_dma_mut!(emu, ARM7).set_dad::<3>(mask, value)),
        (io32(0xDC), |mask, value, emu| io_dma_mut!(emu, ARM7).set_cnt::<3>(mask, value, emu)),
        (io16(0x100), |mask, value, emu| io_timers_mut!(emu, ARM7).set_cnt_l::<0>(mask, value)),
        (io16(0x102), |mask, value, emu| io_timers_mut!(emu, ARM7).set_cnt_h::<0>(mask, value, emu)),
        (io16(0x104), |mask, value, emu| io_timers_mut!(emu, ARM7).set_cnt_l::<1>(mask, value)),
        (io16(0x106), |mask, value, emu| io_timers_mut!(emu, ARM7).set_cnt_h::<1>(mask, value, emu)),
        (io16(0x108), |mask, value, emu| io_timers_mut!(emu, ARM7).set_cnt_l::<2>(mask, value)),
        (io16(0x10A), |mask, value, emu| io_timers_mut!(emu, ARM7).set_cnt_h::<2>(mask, value, emu)),
        (io16(0x10C), |mask, value, emu| io_timers_mut!(emu, ARM7).set_cnt_l::<3>(mask, value)),
        (io16(0x10E), |mask, value, emu| io_timers_mut!(emu, ARM7).set_cnt_h::<3>(mask, value, emu)),
        (io8(0x138), |value, emu| io_rtc_mut!(emu).set_rtc(value)),
        (io16(0x180), |mask, value, emu| get_common_mut!(emu).ipc.set_sync_reg::<{ ARM7 }>(mask, value, emu)),
        (io16(0x184), |mask, value, emu| get_common_mut!(emu).ipc.set_fifo_cnt::<{ ARM7 }>(mask, value, emu)),
        (io32(0x188), |mask, value, emu| get_common_mut!(emu).ipc.fifo_send::<{ ARM7 }>(mask, value, emu)),
        (io16(0x1A0), |mask, value, emu| get_common_mut!(emu).cartridge.set_aux_spi_cnt::<{ ARM7 }>(mask, value)),
        (io8(0x1A2), |value, emu| get_common_mut!(emu).cartridge.set_aux_spi_data::<{ ARM7 }>(value)),
        (io32(0x1A4), |mask, value, emu| get_common_mut!(emu).cartridge.set_rom_ctrl::<{ ARM7 }>(mask, value, emu)),
        (io32(0x1A8), |mask, value, emu| get_common_mut!(emu).cartridge.set_bus_cmd_out_l::<{ ARM7 }>(mask, value)),
        (io32(0x1AC), |mask, value, emu| get_common_mut!(emu).cartridge.set_bus_cmd_out_h::<{ ARM7 }>(mask, value)),
        (io16(0x1C0), |mask, value, emu| io_spi_mut!(emu).set_cnt(mask, value)),
        (io8(0x1C2), |value, emu| io_spi_mut!(emu).set_data(value)),
        (io8(0x208), |value, emu| get_cpu_regs_mut!(emu, ARM7).set_ime(value, emu)),
        (io32(0x210), |mask, value, emu| get_cpu_regs_mut!(emu, ARM7).set_ie(mask, value, emu)),
        (io32(0x214), |mask, value, emu| get_cpu_regs_mut!(emu, ARM7).set_irf(mask, value)),
        (io8(0x300), |value, emu| get_cpu_regs_mut!(emu, ARM7).set_post_flg(value)),
        (io8(0x301), |value, emu| todo!()),
        (io32(0x400), |mask, value, emu| get_spu_mut!(emu).set_cnt(0, mask, value, emu)),
        (io32(0x404), |mask, value, emu| get_spu_mut!(emu).set_sad(0, mask, value, emu)),
        (io16(0x408), |mask, value, emu| get_spu_mut!(emu).set_tmr(0, mask, value)),
        (io16(0x40A), |mask, value, emu| get_spu_mut!(emu).set_pnt(0, mask, value)),
        (io32(0x40C), |mask, value, emu| get_spu_mut!(emu).set_len(0, mask, value)),
        (io32(0x410), |mask, value, emu| get_spu_mut!(emu).set_cnt(1, mask, value, emu)),
        (io32(0x414), |mask, value, emu| get_spu_mut!(emu).set_sad(1, mask, value, emu)),
        (io16(0x418), |mask, value, emu| get_spu_mut!(emu).set_tmr(1, mask, value)),
        (io16(0x41A), |mask, value, emu| get_spu_mut!(emu).set_pnt(1, mask, value)),
        (io32(0x41C), |mask, value, emu| get_spu_mut!(emu).set_len(1, mask, value)),
        (io32(0x420), |mask, value, emu| get_spu_mut!(emu).set_cnt(2, mask, value, emu)),
        (io32(0x424), |mask, value, emu| get_spu_mut!(emu).set_sad(2, mask, value, emu)),
        (io16(0x428), |mask, value, emu| get_spu_mut!(emu).set_tmr(2, mask, value)),
        (io16(0x42A), |mask, value, emu| get_spu_mut!(emu).set_pnt(2, mask, value)),
        (io32(0x42C), |mask, value, emu| get_spu_mut!(emu).set_len(2, mask, value)),
        (io32(0x430), |mask, value, emu| get_spu_mut!(emu).set_cnt(3, mask, value, emu)),
        (io32(0x434), |mask, value, emu| get_spu_mut!(emu).set_sad(3, mask, value, emu)),
        (io16(0x438), |mask, value, emu| get_spu_mut!(emu).set_tmr(3, mask, value)),
        (io16(0x43A), |mask, value, emu| get_spu_mut!(emu).set_pnt(3, mask, value)),
        (io32(0x43C), |mask, value, emu| get_spu_mut!(emu).set_len(3, mask, value)),
        (io32(0x440), |mask, value, emu| get_spu_mut!(emu).set_cnt(4, mask, value, emu)),
        (io32(0x444), |mask, value, emu| get_spu_mut!(emu).set_sad(4, mask, value, emu)),
        (io16(0x448), |mask, value, emu| get_spu_mut!(emu).set_tmr(4, mask, value)),
        (io16(0x44A), |mask, value, emu| get_spu_mut!(emu).set_pnt(4, mask, value)),
        (io32(0x44C), |mask, value, emu| get_spu_mut!(emu).set_len(4, mask, value)),
        (io32(0x450), |mask, value, emu| get_spu_mut!(emu).set_cnt(5, mask, value, emu)),
        (io32(0x454), |mask, value, emu| get_spu_mut!(emu).set_sad(5, mask, value, emu)),
        (io16(0x458), |mask, value, emu| get_spu_mut!(emu).set_tmr(5, mask, value)),
        (io16(0x45A), |mask, value, emu| get_spu_mut!(emu).set_pnt(5, mask, value)),
        (io32(0x45C), |mask, value, emu| get_spu_mut!(emu).set_len(5, mask, value)),
        (io32(0x460), |mask, value, emu| get_spu_mut!(emu).set_cnt(6, mask, value, emu)),
        (io32(0x464), |mask, value, emu| get_spu_mut!(emu).set_sad(6, mask, value, emu)),
        (io16(0x468), |mask, value, emu| get_spu_mut!(emu).set_tmr(6, mask, value)),
        (io16(0x46A), |mask, value, emu| get_spu_mut!(emu).set_pnt(6, mask, value)),
        (io32(0x46C), |mask, value, emu| get_spu_mut!(emu).set_len(6, mask, value)),
        (io32(0x470), |mask, value, emu| get_spu_mut!(emu).set_cnt(7, mask, value, emu)),
        (io32(0x474), |mask, value, emu| get_spu_mut!(emu).set_sad(7, mask, value, emu)),
        (io16(0x478), |mask, value, emu| get_spu_mut!(emu).set_tmr(7, mask, value)),
        (io16(0x47A), |mask, value, emu| get_spu_mut!(emu).set_pnt(7, mask, value)),
        (io32(0x47C), |mask, value, emu| get_spu_mut!(emu).set_len(7, mask, value)),
        (io32(0x480), |mask, value, emu| get_spu_mut!(emu).set_cnt(8, mask, value, emu)),
        (io32(0x484), |mask, value, emu| get_spu_mut!(emu).set_sad(8, mask, value, emu)),
        (io16(0x488), |mask, value, emu| get_spu_mut!(emu).set_tmr(8, mask, value)),
        (io16(0x48A), |mask, value, emu| get_spu_mut!(emu).set_pnt(8, mask, value)),
        (io32(0x48C), |mask, value, emu| get_spu_mut!(emu).set_len(8, mask, value)),
        (io32(0x490), |mask, value, emu| get_spu_mut!(emu).set_cnt(9, mask, value, emu)),
        (io32(0x494), |mask, value, emu| get_spu_mut!(emu).set_sad(9, mask, value, emu)),
        (io16(0x498), |mask, value, emu| get_spu_mut!(emu).set_tmr(9, mask, value)),
        (io16(0x49A), |mask, value, emu| get_spu_mut!(emu).set_pnt(9, mask, value)),
        (io32(0x49C), |mask, value, emu| get_spu_mut!(emu).set_len(9, mask, value)),
        (io32(0x4A0), |mask, value, emu| get_spu_mut!(emu).set_cnt(10, mask, value, emu)),
        (io32(0x4A4), |mask, value, emu| get_spu_mut!(emu).set_sad(10, mask, value, emu)),
        (io16(0x4A8), |mask, value, emu| get_spu_mut!(emu).set_tmr(10, mask, value)),
        (io16(0x4AA), |mask, value, emu| get_spu_mut!(emu).set_pnt(10, mask, value)),
        (io32(0x4AC), |mask, value, emu| get_spu_mut!(emu).set_len(10, mask, value)),
        (io32(0x4B0), |mask, value, emu| get_spu_mut!(emu).set_cnt(11, mask, value, emu)),
        (io32(0x4B4), |mask, value, emu| get_spu_mut!(emu).set_sad(11, mask, value, emu)),
        (io16(0x4B8), |mask, value, emu| get_spu_mut!(emu).set_tmr(11, mask, value)),
        (io16(0x4BA), |mask, value, emu| get_spu_mut!(emu).set_pnt(11, mask, value)),
        (io32(0x4BC), |mask, value, emu| get_spu_mut!(emu).set_len(11, mask, value)),
        (io32(0x4C0), |mask, value, emu| get_spu_mut!(emu).set_cnt(12, mask, value, emu)),
        (io32(0x4C4), |mask, value, emu| get_spu_mut!(emu).set_sad(12, mask, value, emu)),
        (io16(0x4C8), |mask, value, emu| get_spu_mut!(emu).set_tmr(12, mask, value)),
        (io16(0x4CA), |mask, value, emu| get_spu_mut!(emu).set_pnt(12, mask, value)),
        (io32(0x4CC), |mask, value, emu| get_spu_mut!(emu).set_len(12, mask, value)),
        (io32(0x4D0), |mask, value, emu| get_spu_mut!(emu).set_cnt(13, mask, value, emu)),
        (io32(0x4D4), |mask, value, emu| get_spu_mut!(emu).set_sad(13, mask, value, emu)),
        (io16(0x4D8), |mask, value, emu| get_spu_mut!(emu).set_tmr(13, mask, value)),
        (io16(0x4DA), |mask, value, emu| get_spu_mut!(emu).set_pnt(13, mask, value)),
        (io32(0x4DC), |mask, value, emu| get_spu_mut!(emu).set_len(13, mask, value)),
        (io32(0x4E0), |mask, value, emu| get_spu_mut!(emu).set_cnt(14, mask, value, emu)),
        (io32(0x4E4), |mask, value, emu| get_spu_mut!(emu).set_sad(14, mask, value, emu)),
        (io16(0x4E8), |mask, value, emu| get_spu_mut!(emu).set_tmr(14, mask, value)),
        (io16(0x4EA), |mask, value, emu| get_spu_mut!(emu).set_pnt(14, mask, value)),
        (io32(0x4EC), |mask, value, emu| get_spu_mut!(emu).set_len(14, mask, value)),
        (io32(0x4F0), |mask, value, emu| get_spu_mut!(emu).set_cnt(15, mask, value, emu)),
        (io32(0x4F4), |mask, value, emu| get_spu_mut!(emu).set_sad(15, mask, value, emu)),
        (io16(0x4F8), |mask, value, emu| get_spu_mut!(emu).set_tmr(15, mask, value)),
        (io16(0x4FA), |mask, value, emu| get_spu_mut!(emu).set_pnt(15, mask, value)),
        (io32(0x4FC), |mask, value, emu| get_spu_mut!(emu).set_len(15, mask, value)),
        (io16(0x500), |mask, value, emu| get_spu_mut!(emu).set_main_sound_cnt(mask, value, emu)),
        (io16(0x504), |mask, value, emu| get_spu_mut!(emu).set_sound_bias(mask, value)),
        (io8(0x508), |value, emu| get_spu_mut!(emu).set_snd_cap_cnt(0, value)),
        (io8(0x509), |value, emu| get_spu_mut!(emu).set_snd_cap_cnt(1, value)),
        (io32(0x510), |mask, value, emu| get_spu_mut!(emu).set_snd_cap_dad(0, mask, value)),
        (io16(0x514), |mask, value, emu| get_spu_mut!(emu).set_snd_cap_len(0, mask, value)),
        (io32(0x518), |mask, value, emu| get_spu_mut!(emu).set_snd_cap_dad(1, mask, value)),
        (io16(0x51C), |mask, value, emu| get_spu_mut!(emu).set_snd_cap_len(1, mask, value)),
    ]
);

io_write!(
    IoArm7WriteLutWifi,
    [
        (io16(0x800006), |mask, value, emu| io_wifi_mut!(emu).set_w_mode_wep(mask, value)),
        (io16(0x800008), |mask, value, emu| io_wifi_mut!(emu).set_w_txstat_cnt(mask, value)),
        (io16(0x800010), |mask, value, emu| io_wifi_mut!(emu).set_w_irf(mask, value)),
        (io16(0x800012), |mask, value, emu| io_wifi_mut!(emu).set_w_ie(mask, value, emu)),
        (io16(0x800018), |mask, value, emu| io_wifi_mut!(emu).set_w_macaddr(0, mask, value)),
        (io16(0x80001A), |mask, value, emu| io_wifi_mut!(emu).set_w_macaddr(1, mask, value)),
        (io16(0x80001C), |mask, value, emu| io_wifi_mut!(emu).set_w_macaddr(2, mask, value)),
        (io16(0x800020), |mask, value, emu| io_wifi_mut!(emu).set_w_bssid(0, mask, value)),
        (io16(0x800022), |mask, value, emu| io_wifi_mut!(emu).set_w_bssid(1, mask, value)),
        (io16(0x800024), |mask, value, emu| io_wifi_mut!(emu).set_w_bssid(2, mask, value)),
        (io16(0x80002A), |mask, value, emu| io_wifi_mut!(emu).set_w_aid_full(mask, value)),
        (io16(0x800030), |mask, value, emu| io_wifi_mut!(emu).set_w_rxcnt(mask, value)),
        (io16(0x80003C), |mask, value, emu| io_wifi_mut!(emu).set_w_powerstate(mask, value)),
        (io16(0x800040), |mask, value, emu| io_wifi_mut!(emu).set_w_powerforce(mask, value)),
        (io16(0x800050), |mask, value, emu| io_wifi_mut!(emu).set_w_rxbuf_begin(mask, value)),
        (io16(0x800052), |mask, value, emu| io_wifi_mut!(emu).set_w_rxbuf_end(mask, value)),
        (io16(0x800056), |mask, value, emu| io_wifi_mut!(emu).set_w_rxbuf_wr_addr(mask, value)),
        (io16(0x800058), |mask, value, emu| io_wifi_mut!(emu).set_w_rxbuf_rd_addr(mask, value)),
        (io16(0x80005A), |mask, value, emu| io_wifi_mut!(emu).set_w_rxbuf_readcsr(mask, value)),
        (io16(0x80005C), |mask, value, emu| io_wifi_mut!(emu).set_w_rxbuf_count(mask, value)),
        (io16(0x800062), |mask, value, emu| io_wifi_mut!(emu).set_w_rxbuf_gap(mask, value)),
        (io16(0x800064), |mask, value, emu| io_wifi_mut!(emu).set_w_rxbuf_gapdisp(mask, value)),
        (io16(0x800068), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_wr_addr(mask, value)),
        (io16(0x80006C), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_count(mask, value)),
        (io16(0x800070), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_wr_data(mask, value, emu)),
        (io16(0x800074), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_gap(mask, value)),
        (io16(0x800076), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_gapdisp(mask, value)),
        (io16(0x800080), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_loc(PaketType::BeaconFrame, mask, value)),
        (io16(0x80008C), |mask, value, emu| io_wifi_mut!(emu).set_w_beacon_int(mask, value)),
        (io16(0x800090), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_loc(PaketType::CmdFrame, mask, value)),
        (io16(0x800094), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_reply1(mask, value)),
        (io16(0x8000A0), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_loc(PaketType::Loc1Frame, mask, value)),
        (io16(0x8000A4), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_loc(PaketType::Loc2Frame, mask, value)),
        (io16(0x8000A8), |mask, value, emu| io_wifi_mut!(emu).set_w_txbuf_loc(PaketType::Loc3Frame, mask, value)),
        (io16(0x8000AC), |mask, value, emu| io_wifi_mut!(emu).set_w_txreq_reset(mask, value)),
        (io16(0x8000AE), |mask, value, emu| io_wifi_mut!(emu).set_w_txreq_set(mask, value)),
        (io16(0x8000E8), |mask, value, emu| io_wifi_mut!(emu).set_w_us_countcnt(mask, value)),
        (io16(0x8000EA), |mask, value, emu| io_wifi_mut!(emu).set_w_us_comparecnt(mask, value)),
        (io16(0x8000EE), |mask, value, emu| io_wifi_mut!(emu).set_w_cmd_countcnt(mask, value)),
        (io16(0x8000F0), |mask, value, emu| io_wifi_mut!(emu).set_w_us_compare(0, mask, value)),
        (io16(0x8000F2), |mask, value, emu| io_wifi_mut!(emu).set_w_us_compare(1, mask, value)),
        (io16(0x8000F4), |mask, value, emu| io_wifi_mut!(emu).set_w_us_compare(2, mask, value)),
        (io16(0x8000F6), |mask, value, emu| io_wifi_mut!(emu).set_w_us_compare(3, mask, value)),
        (io16(0x8000F8), |mask, value, emu| io_wifi_mut!(emu).set_w_us_count(0, mask, value)),
        (io16(0x8000FA), |mask, value, emu| io_wifi_mut!(emu).set_w_us_count(1, mask, value)),
        (io16(0x8000FC), |mask, value, emu| io_wifi_mut!(emu).set_w_us_count(2, mask, value)),
        (io16(0x8000FE), |mask, value, emu| io_wifi_mut!(emu).set_w_us_count(3, mask, value)),
        (io16(0x800110), |mask, value, emu| io_wifi_mut!(emu).set_w_pre_beacon(mask, value)),
        (io16(0x800118), |mask, value, emu| io_wifi_mut!(emu).set_w_cmd_count(mask, value)),
        (io16(0x80011C), |mask, value, emu| io_wifi_mut!(emu).set_w_beacon_count(mask, value)),
        (io16(0x800120), |mask, value, emu| io_wifi_mut!(emu).set_w_config(0, mask, value)),
        (io16(0x800122), |mask, value, emu| io_wifi_mut!(emu).set_w_config(1, mask, value)),
        (io16(0x800124), |mask, value, emu| io_wifi_mut!(emu).set_w_config(2, mask, value)),
        (io16(0x800128), |mask, value, emu| io_wifi_mut!(emu).set_w_config(3, mask, value)),
        (io16(0x800130), |mask, value, emu| io_wifi_mut!(emu).set_w_config(4, mask, value)),
        (io16(0x800132), |mask, value, emu| io_wifi_mut!(emu).set_w_config(5, mask, value)),
        (io16(0x800134), |mask, value, emu| io_wifi_mut!(emu).set_w_post_beacon(mask, value)),
        (io16(0x800140), |mask, value, emu| io_wifi_mut!(emu).set_w_config(6, mask, value)),
        (io16(0x800142), |mask, value, emu| io_wifi_mut!(emu).set_w_config(7, mask, value)),
        (io16(0x800144), |mask, value, emu| io_wifi_mut!(emu).set_w_config(8, mask, value)),
        (io16(0x800146), |mask, value, emu| io_wifi_mut!(emu).set_w_config(9, mask, value)),
        (io16(0x800148), |mask, value, emu| io_wifi_mut!(emu).set_w_config(10, mask, value)),
        (io16(0x80014A), |mask, value, emu| io_wifi_mut!(emu).set_w_config(11, mask, value)),
        (io16(0x80014C), |mask, value, emu| io_wifi_mut!(emu).set_w_config(12, mask, value)),
        (io16(0x800150), |mask, value, emu| io_wifi_mut!(emu).set_w_config(13, mask, value)),
        (io16(0x800154), |mask, value, emu| io_wifi_mut!(emu).set_w_config(14, mask, value)),
        (io16(0x800158), |mask, value, emu| io_wifi_mut!(emu).set_w_bb_cnt(mask, value)),
        (io16(0x80015A), |mask, value, emu| io_wifi_mut!(emu).set_w_bb_write(mask, value)),
        (io16(0x80021C), |mask, value, emu| io_wifi_mut!(emu).set_w_irf_set(mask, value, emu)),
    ]
);

pub mod Arm7Io {
    use crate::core::graphics::gpu::DispStat;
    use crate::core::ipc::{IpcFifoCnt, IpcSyncCnt};
    use crate::core::memory::cartridge::{AuxSpiCnt, RomCtrl};
    use crate::core::rtc::RtcReg;
    use crate::core::spi::SpiCnt;
    use crate::core::timers::TimerCntH;
    use dsvita_macros::io;

    io!(
        (
            io_0! {
                use crate::core::CpuType::ARM7;
            },
            (io16(0x4, DispStat), gpu_disp_stat, || {}, |emu| emu.gpu_set_disp_stat(ARM7)),
            (io16(0x6), gpu_v_count),
            (io32(0xB0), dma_sad_0, || {}, |emu| emu.dma_set_sad(ARM7, 0)),
            (io32(0xB4), dma_dad_0, || {}, |emu| emu.dma_set_dad(ARM7, 0)),
            (io32(0xB8), dma_cnt_0, || {}, |emu| emu.dma_set_cnt(ARM7, 0)),
            (io32(0xBC), dma_sad_1, || {}, |emu| emu.dma_set_sad(ARM7, 1)),
            (io32(0xC0), dma_dad_1, || {}, |emu| emu.dma_set_dad(ARM7, 1)),
            (io32(0xC4), dma_cnt_1, || {}, |emu| emu.dma_set_cnt(ARM7, 1)),
            (io32(0xC8), dma_sad_2, || {}, |emu| emu.dma_set_sad(ARM7, 2)),
            (io32(0xCC), dma_dad_2, || {}, |emu| emu.dma_set_dad(ARM7, 2)),
            (io32(0xD0), dma_cnt_2, || {}, |emu| emu.dma_set_cnt(ARM7, 2)),
            (io32(0xD4), dma_sad_3, || {}, |emu| emu.dma_set_sad(ARM7, 3)),
            (io32(0xD8), dma_dad_3, || {}, |emu| emu.dma_set_dad(ARM7, 3)),
            (io32(0xDC), dma_cnt_3, || {}, |emu| emu.dma_set_cnt(ARM7, 3)),
        ),
        (
            io_1! {
                use crate::core::CpuType::ARM7;
            },
            (io16(0x100), timers_cnt_l_0, |emu| emu.timers_get_cnt_l(ARM7, 0), |emu| emu.timers_set_cnt_l(ARM7, 0)),
            (io8(0x102, TimerCntH), timers_cnt_h_0, || {}, |emu| emu.timers_set_cnt_h(ARM7, 0)),
            (io16(0x104), timers_cnt_l_1, |emu| emu.timers_get_cnt_l(ARM7, 1), |emu| emu.timers_set_cnt_l(ARM7, 1)),
            (io8(0x106, TimerCntH), timers_cnt_h_1, || {}, |emu| emu.timers_set_cnt_h(ARM7, 1)),
            (io16(0x108), timers_cnt_l_2, |emu| emu.timers_get_cnt_l(ARM7, 2), |emu| emu.timers_set_cnt_l(ARM7, 2)),
            (io8(0x10A, TimerCntH), timers_cnt_h_2, || {}, |emu| emu.timers_set_cnt_h(ARM7, 2)),
            (io16(0x10C), timers_cnt_l_3, |emu| emu.timers_get_cnt_l(ARM7, 3), |emu| emu.timers_set_cnt_l(ARM7, 3)),
            (io8(0x10E, TimerCntH), timers_cnt_h_3, || {}, |emu| emu.timers_set_cnt_h(ARM7, 3)),
            (io16(0x130), inout_key, |emu| emu.input.get_key_input()),
            (io16(0x136), input_ext_key_in, |emu| emu.input.get_ext_key_in()),
            (io8(0x138, RtcReg), rtc, |emu| emu.rtc_get(), |emu| emu.rtc_set()),
            (io16(0x180, IpcSyncCnt), ipc_sync_reg, || {}, |emu| emu.ipc_set_sync_reg(ARM7)),
            (io16(0x184, IpcFifoCnt), ipc_fifo_cnt, |emu| emu.ipc_get_fifo_cnt(ARM7), |emu| emu.ipc_set_fifo_cnt(ARM7)),
            (io32(0x188), ipc_fifo_send, || {}, |emu| emu.ipc_fifo_send(ARM7)),
            (io16(0x1A0, AuxSpiCnt), cartridge_aux_spi_cnt, || {}, |emu| emu.cartridge_set_aux_spi_cnt(ARM7)),
            (io8(0x1A2), cartridge_aux_spi_data, || {}, |emu| emu.cartridge_set_aux_spi_data(ARM7)),
            (io32(0x1A4, RomCtrl), cartridge_rom_ctrl, |emu| emu.cartridge_get_rom_ctrl(ARM7), |emu| emu.cartridge_set_rom_ctrl(ARM7)),
            (io32(0x1A8), cartridge_bus_cmd_out_l),
            (io32(0x1AC), cartridge_bus_cmd_out_h),
            (io16(0x1C0, SpiCnt), spi_cnt, || {}, |emu| emu.spi_set_cnt()),
            (io8(0x1C2), spi_data, || {}, |emu| emu.spi_set_data()),
        ),
        (
            io_2! {
                use crate::core::CpuType::ARM7;
            },
            (io8(0x208), cpu_ime, || {}, |emu| emu.cpu_set_ime(ARM7)),
            (io32(0x210), cpu_ie, || {}, |emu| emu.cpu_set_ie(ARM7)),
            (io32(0x214), cpu_irf),
            (io8(0x240), vram_stat),
            (io8(0x241), wram_cnt),
        ),
        (
            io_3! {
                use crate::core::CpuType::ARM7;
            },
            (io8(0x300), cpu_post_flg, || {}, |emu| emu.cpu_set_post_flg(ARM7)),
            (io8(0x301), halt_cnt, || {}, |emu| todo!()),
        ),
        (
            io_4! {},
            (io32(0x400), spu_cnt_0, |emu| emu.spu_get_io_cnt(0), |emu| emu.spu_set_io_cnt(0)),
            (io32(0x404), spu_sad_0, || {}, |emu| emu.spu_set_io_sad(0)),
            (io16(0x408), spu_tmr_0),
            (io16(0x40A), spu_pnt_0),
            (io32(0x40C), spu_len_0, || {}, |emu| emu.spu_set_io_len(0)),
            (io32(0x410), spu_cnt_1, |emu| emu.spu_get_io_cnt(1), |emu| emu.spu_set_io_cnt(1)),
            (io32(0x414), spu_sad_1, || {}, |emu| emu.spu_set_io_sad(1)),
            (io16(0x418), spu_tmr_1),
            (io16(0x41A), spu_pnt_1),
            (io32(0x41C), spu_len_1, || {}, |emu| emu.spu_set_io_len(1)),
            (io32(0x420), spu_cnt_2, |emu| emu.spu_get_io_cnt(2), |emu| emu.spu_set_io_cnt(2)),
            (io32(0x424), spu_sad_2, || {}, |emu| emu.spu_set_io_sad(2)),
            (io16(0x428), spu_tmr_2),
            (io16(0x42A), spu_pnt_2),
            (io32(0x42C), spu_len_2, || {}, |emu| emu.spu_set_io_len(2)),
            (io32(0x430), spu_cnt_3, |emu| emu.spu_get_io_cnt(3), |emu| emu.spu_set_io_cnt(3)),
            (io32(0x434), spu_sad_3, || {}, |emu| emu.spu_set_io_sad(3)),
            (io16(0x438), spu_tmr_3),
            (io16(0x43A), spu_pnt_3),
            (io32(0x43C), spu_len_3, || {}, |emu| emu.spu_set_io_len(3)),
            (io32(0x440), spu_cnt_4, |emu| emu.spu_get_io_cnt(4), |emu| emu.spu_set_io_cnt(4)),
            (io32(0x444), spu_sad_4, || {}, |emu| emu.spu_set_io_sad(4)),
            (io16(0x448), spu_tmr_4),
            (io16(0x44A), spu_pnt_4),
            (io32(0x44C), spu_len_4, || {}, |emu| emu.spu_set_io_len(4)),
            (io32(0x450), spu_cnt_5, |emu| emu.spu_get_io_cnt(5), |emu| emu.spu_set_io_cnt(5)),
            (io32(0x454), spu_sad_5, || {}, |emu| emu.spu_set_io_sad(5)),
            (io16(0x458), spu_tmr_5),
            (io16(0x45A), spu_pnt_5),
            (io32(0x45C), spu_len_5, || {}, |emu| emu.spu_set_io_len(5)),
            (io32(0x460), spu_cnt_6, |emu| emu.spu_get_io_cnt(6), |emu| emu.spu_set_io_cnt(6)),
            (io32(0x464), spu_sad_6, || {}, |emu| emu.spu_set_io_sad(6)),
            (io16(0x468), spu_tmr_6),
            (io16(0x46A), spu_pnt_6),
            (io32(0x46C), spu_len_6, || {}, |emu| emu.spu_set_io_len(6)),
            (io32(0x470), spu_cnt_7, |emu| emu.spu_get_io_cnt(7), |emu| emu.spu_set_io_cnt(7)),
            (io32(0x474), spu_sad_7, || {}, |emu| emu.spu_set_io_sad(7)),
            (io16(0x478), spu_tmr_7),
            (io16(0x47A), spu_pnt_7),
            (io32(0x47C), spu_len_7, || {}, |emu| emu.spu_set_io_len(7)),
            (io32(0x480), spu_cnt_8, |emu| emu.spu_get_io_cnt(8), |emu| emu.spu_set_io_cnt(8)),
            (io32(0x484), spu_sad_8, || {}, |emu| emu.spu_set_io_sad(8)),
            (io16(0x488), spu_tmr_8),
            (io16(0x48A), spu_pnt_8),
            (io32(0x48C), spu_len_8, || {}, |emu| emu.spu_set_io_len(8)),
            (io32(0x490), spu_cnt_9, |emu| emu.spu_get_io_cnt(9), |emu| emu.spu_set_io_cnt(9)),
            (io32(0x494), spu_sad_9, || {}, |emu| emu.spu_set_io_sad(9)),
            (io16(0x498), spu_tmr_9),
            (io16(0x49A), spu_pnt_9),
            (io32(0x49C), spu_len_9, || {}, |emu| emu.spu_set_io_len(9)),
            (io32(0x4A0), spu_cnt_10, |emu| emu.spu_get_io_cnt(10), |emu| emu.spu_set_io_cnt(10)),
            (io32(0x4A4), spu_sad_10, || {}, |emu| emu.spu_set_io_sad(10)),
            (io16(0x4A8), spu_tmr_10),
            (io16(0x4AA), spu_pnt_10),
            (io32(0x4AC), spu_len_10, || {}, |emu| emu.spu_set_io_len(10)),
            (io32(0x4B0), spu_cnt_11, |emu| emu.spu_get_io_cnt(11), |emu| emu.spu_set_io_cnt(11)),
            (io32(0x4B4), spu_sad_11, || {}, |emu| emu.spu_set_io_sad(11)),
            (io16(0x4B8), spu_tmr_11),
            (io16(0x4BA), spu_pnt_11),
            (io32(0x4BC), spu_len_11, || {}, |emu| emu.spu_set_io_len(11)),
            (io32(0x4C0), spu_cnt_12, |emu| emu.spu_get_io_cnt(12), |emu| emu.spu_set_io_cnt(12)),
            (io32(0x4C4), spu_sad_12, || {}, |emu| emu.spu_set_io_sad(12)),
            (io16(0x4C8), spu_tmr_12),
            (io16(0x4CA), spu_pnt_12),
            (io32(0x4CC), spu_len_12, || {}, |emu| emu.spu_set_io_len(12)),
            (io32(0x4D0), spu_cnt_13, |emu| emu.spu_get_io_cnt(13), |emu| emu.spu_set_io_cnt(13)),
            (io32(0x4D4), spu_sad_13, || {}, |emu| emu.spu_set_io_sad(13)),
            (io16(0x4D8), spu_tmr_13),
            (io16(0x4DA), spu_pnt_13),
            (io32(0x4DC), spu_len_13, || {}, |emu| emu.spu_set_io_len(13)),
            (io32(0x4E0), spu_cnt_14, |emu| emu.spu_get_io_cnt(14), |emu| emu.spu_set_io_cnt(14)),
            (io32(0x4E4), spu_sad_14, || {}, |emu| emu.spu_set_io_sad(14)),
            (io16(0x4E8), spu_tmr_14),
            (io16(0x4EA), spu_pnt_14),
            (io32(0x4EC), spu_len_14, || {}, |emu| emu.spu_set_io_len(14)),
            (io32(0x4F0), spu_cnt_15, |emu| emu.spu_get_io_cnt(15), |emu| emu.spu_set_io_cnt(15)),
            (io32(0x4F4), spu_sad_15, || {}, |emu| emu.spu_set_io_sad(15)),
            (io16(0x4F8), spu_tmr_15),
            (io16(0x4FA), spu_pnt_15),
            (io32(0x4FC), spu_len_15, || {}, |emu| emu.spu_set_io_len(15)),
        ),
        (
            io_5! {},
            (io16(0x500), spu_main_sound_cnt, || {}, |emu| emu.spu_set_io_main_sound_cnt()),
            (io16(0x504), spu_sound_bias, || {}, |emu| emu.spu_set_io_sound_bias()),
            (io8(0x508), spu_snd_cap_cnt_0, |emu| emu.spu_get_io_snd_cap_cnt(0), |emu| emu.spu_set_io_snd_cap_cnt(0)),
            (io8(0x509), spu_snd_cap_cnt_1, |emu| emu.spu_get_io_snd_cap_cnt(1), |emu| emu.spu_set_io_snd_cap_cnt(1)),
            (io32(0x510), spu_snd_cap_dad_0, || {}, |emu| emu.spu_set_io_snd_cap_dad(0)),
            (io16(0x514), spu_snd_cap_len_0),
            (io32(0x518), spu_snd_cap_dad_1, || {}, |emu| emu.spu_set_io_snd_cap_dad(1)),
            (io16(0x51C), spu_snd_cap_len_1),
        ),
        (
            io_upper! {
                use crate::core::CpuType::ARM7;
            },
            (io32(0x100000), ipc_fifo_recv, |emu| emu.ipc_fifo_recv(ARM7)),
            (io32(0x100010), cartridge_rom_data_in, |emu| emu.cartridge_get_rom_data_in(ARM7)),
        ),
        // (
        //     io_wifi! {},
        //     (io16(0x800006), wifi_mode_wep),
        //     (io16(0x800008), wifi_txstat_cnt, || {}, |emu| emu.wifi_set_w_txstat_cnt()),
        //     (io16(0x800010), wifi_irf, || {}, |emu| emu.wifi_set_w_irf()),
        //     (io16(0x800012), wifi_ie, || {}, |emu| emu.wifi_set_w_ie(mask, value)),
        //     (io16(0x800018), wifi_macaddr_0, || {}, |emu| emu.wifi_set_w_macaddr(0)),
        //     (io16(0x80001A), wifi_macaddr_1, || {}, |emu| emu.wifi_set_w_macaddr(1)),
        //     (io16(0x80001C), wifi_macaddr_2, || {}, |emu| emu.wifi_set_w_macaddr(2)),
        //     (io16(0x800020), wifi_bssid_0, || {}, |emu| emu.wifi_set_w_bssid(0)),
        //     (io16(0x800022), wifi_bssid_1, || {}, |emu| emu.wifi_set_w_bssid(1)),
        //     (io16(0x800024), wifi_bssid_2, || {}, |emu| emu.wifi_set_w_bssid(2)),
        //     (io16(0x80002A), wifi_aid_full, || {}, |emu| emu.wifi_set_w_aid_full(mask, value)),
        //     (io16(0x800030), wifi_rxcnt, || {}, |emu| emu.wifi_set_w_rxcnt(mask, value)),
        //     (io16(0x80003C), wifi_powerstate, || {}, |emu| emu.wifi_set_w_powerstate(mask, value)),
        //     (io16(0x800040), wifi_powerforce, || {}, |emu| emu.wifi_set_w_powerforce(mask, value)),
        //     (io16(0x800050), wifi_rxbuf_begin, || {}, |emu| emu.wifi_set_w_rxbuf_begin(mask, value)),
        //     (io16(0x800052), wifi_rxbuf_end, || {}, |emu| emu.wifi_set_w_rxbuf_end(mask, value)),
        //     (io16(0x800056), wifi_rxbuf_wr_addr, || {}, |emu| emu.wifi_set_w_rxbuf_wr_addr(mask, value)),
        //     (io16(0x800058), wifi_rxbuf_rd_addr, || {}, |emu| emu.wifi_set_w_rxbuf_rd_addr(mask, value)),
        //     (io16(0x80005A), wifi_rxbuf_readcsr, || {}, |emu| emu.wifi_set_w_rxbuf_readcsr(mask, value)),
        //     (io16(0x80005C), wifi_rxbuf_count, || {}, |emu| emu.wifi_set_w_rxbuf_count(mask, value)),
        //     (io16(0x800062), wifi_rxbuf_gap, || {}, |emu| emu.wifi_set_w_rxbuf_gap(mask, value)),
        //     (io16(0x800064), wifi_rxbuf_gapdisp, || {}, |emu| emu.wifi_set_w_rxbuf_gapdisp(mask, value)),
        //     (io16(0x800068), wifi_txbuf_wr_addr, || {}, |emu| emu.wifi_set_w_txbuf_wr_addr(mask, value)),
        //     (io16(0x80006C), wifi_txbuf_count, || {}, |emu| emu.wifi_set_w_txbuf_count(mask, value)),
        //     (io16(0x800070), wifi_txbuf_wr_data, || {}, |emu| emu.wifi_set_w_txbuf_wr_data(mask, value)),
        //     (io16(0x800074), wifi_txbuf_gap, || {}, |emu| emu.wifi_set_w_txbuf_gap(mask, value)),
        //     (io16(0x800076), wifi_txbuf_gapdisp, || {}, |emu| emu.wifi_set_w_txbuf_gapdisp(mask, value)),
        //     (io16(0x800080), wifi_txbuf_loc_beacon, || {}, |emu| emu.wifi_set_w_txbuf_loc(PaketType::BeaconFrame)),
        //     (io16(0x80008C), wifi_beacon_int, || {}, |emu| emu.wifi_set_w_beacon_int(mask, value)),
        //     (io16(0x800090), wifi_txbuf_loc_cmd, || {}, |emu| emu.wifi_set_w_txbuf_loc(PaketType::CmdFrame)),
        //     (io16(0x800094), wifi_txbuf_reply1, || {}, |emu| emu.wifi_set_w_txbuf_reply1(mask, value)),
        //     (io16(0x8000A0), wifi_txbuf_loc_1, || {}, |emu| emu.wifi_set_w_txbuf_loc(PaketType::Loc1Frame)),
        //     (io16(0x8000A4), wifi_txbuf_loc_2, || {}, |emu| emu.wifi_set_w_txbuf_loc(PaketType::Loc2Frame)),
        //     (io16(0x8000A8), wifi_txbuf_loc_3, || {}, |emu| emu.wifi_set_w_txbuf_loc(PaketType::Loc3Frame)),
        //     (io16(0x8000AC), wifi_txreq_reset, || {}, |emu| emu.wifi_set_w_txreq_reset(mask, value)),
        //     (io16(0x8000AE), wifi_txreq_set, || {}, |emu| emu.wifi_set_w_txreq_set(mask, value)),
        //     (io16(0x8000E8), wifi_us_countcnt, || {}, |emu| emu.wifi_set_w_us_countcnt(mask, value)),
        //     (io16(0x8000EA), wifi_us_comparecnt, || {}, |emu| emu.wifi_set_w_us_comparecnt(mask, value)),
        //     (io16(0x8000EE), wifi_cmd_countcnt, || {}, |emu| emu.wifi_set_w_cmd_countcnt(mask, value)),
        //     (io16(0x8000F0), wifi_us_compare_0, || {}, |emu| emu.wifi_set_w_us_compare(0)),
        //     (io16(0x8000F2), wifi_us_compare_1, || {}, |emu| emu.wifi_set_w_us_compare(1)),
        //     (io16(0x8000F4), wifi_us_compare_2, || {}, |emu| emu.wifi_set_w_us_compare(2)),
        //     (io16(0x8000F6), wifi_us_compare_3, || {}, |emu| emu.wifi_set_w_us_compare(3)),
        //     (io16(0x8000F8), wifi_us_count_0, || {}, |emu| emu.wifi_set_w_us_count(0)),
        //     (io16(0x8000FA), wifi_us_count_1, || {}, |emu| emu.wifi_set_w_us_count(1)),
        //     (io16(0x8000FC), wifi_us_count_2, || {}, |emu| emu.wifi_set_w_us_count(2)),
        //     (io16(0x8000FE), wifi_us_count_3, || {}, |emu| emu.wifi_set_w_us_count(3)),
        //     (io16(0x800110), wifi_pre_beacon, || {}, |emu| emu.wifi_set_w_pre_beacon(mask, value)),
        //     (io16(0x800118), wifi_cmd_count, || {}, |emu| emu.wifi_set_w_cmd_count(mask, value)),
        //     (io16(0x80011C), wifi_beacon_count, || {}, |emu| emu.wifi_set_w_beacon_count(mask, value)),
        //     (io16(0x800120), wifi_config_0, || {}, |emu| emu.wifi_set_w_config(0)),
        //     (io16(0x800122), wifi_config_1, || {}, |emu| emu.wifi_set_w_config(1)),
        //     (io16(0x800124), wifi_config_2, || {}, |emu| emu.wifi_set_w_config(2)),
        //     (io16(0x800128), wifi_config_3, || {}, |emu| emu.wifi_set_w_config(3)),
        //     (io16(0x800130), wifi_config_4, || {}, |emu| emu.wifi_set_w_config(4)),
        //     (io16(0x800132), wifi_config_5, || {}, |emu| emu.wifi_set_w_config(5)),
        //     (io16(0x800134), wifi_post_beacon, || {}, |emu| emu.wifi_set_w_post_beacon(mask, value)),
        //     (io16(0x800140), wifi_config_6, || {}, |emu| emu.wifi_set_w_config(6)),
        //     (io16(0x800142), wifi_config_7, || {}, |emu| emu.wifi_set_w_config(7)),
        //     (io16(0x800144), wifi_config_8, || {}, |emu| emu.wifi_set_w_config(8)),
        //     (io16(0x800146), wifi_config_9, || {}, |emu| emu.wifi_set_w_config(9)),
        //     (io16(0x800148), wifi_config_10, || {}, |emu| emu.wifi_set_w_config(10)),
        //     (io16(0x80014A), wifi_config_11, || {}, |emu| emu.wifi_set_w_config(11)),
        //     (io16(0x80014C), wifi_config_12, || {}, |emu| emu.wifi_set_w_config(12)),
        //     (io16(0x800150), wifi_config_13, || {}, |emu| emu.wifi_set_w_config(13)),
        //     (io16(0x800154), wifi_config_14, || {}, |emu| emu.wifi_set_w_config(14)),
        //     (io16(0x800158), wifi_bb_cnt, || {}, |emu| emu.wifi_set_w_bb_cnt(mask, value)),
        //     (io16(0x80015A), wifi_bb_write, || {}, |emu| emu.wifi_set_w_bb_write(mask, value)),
        //     (io16(0x80015C), wifi_bb_read),
        //     (io16(0x800210), wifi_tx_seqno),
        //     (io16(0x80021C), wifi_irf_set, || {}, |emu| emu.wifi_set_w_irf_set(mask, value)),
        // ),
    );

    impl Memory {
        pub fn spu_cnt(&mut self, channel_num: usize) -> &mut u32 {
            let ptr = std::ptr::addr_of_mut!(self.spu_cnt_0);
            unsafe { ptr.add(channel_num * 4).as_mut_unchecked() }
        }

        pub fn spu_sad(&mut self, channel_num: usize) -> &mut u32 {
            let ptr = std::ptr::addr_of_mut!(self.spu_sad_0);
            unsafe { ptr.add(channel_num * 4).as_mut_unchecked() }
        }

        pub fn spu_tmr(&mut self, channel_num: usize) -> &mut u16 {
            let ptr = std::ptr::addr_of_mut!(self.spu_tmr_0);
            unsafe { ptr.add(channel_num * 8).as_mut_unchecked() }
        }

        pub fn spu_pnt(&mut self, channel_num: usize) -> &mut u16 {
            let ptr = std::ptr::addr_of_mut!(self.spu_pnt_0);
            unsafe { ptr.add(channel_num * 8).as_mut_unchecked() }
        }

        pub fn spu_len(&mut self, channel_num: usize) -> &mut u32 {
            let ptr = std::ptr::addr_of_mut!(self.spu_len_0);
            unsafe { ptr.add(channel_num * 4).as_mut_unchecked() }
        }

        pub fn spu_snd_cap_cnt(&mut self, channel_num: usize) -> &mut u8 {
            let ptr = std::ptr::addr_of_mut!(self.spu_snd_cap_cnt_0);
            unsafe { ptr.add(channel_num).as_mut_unchecked() }
        }

        pub fn spu_snd_cap_dad(&mut self, channel_num: usize) -> &mut u32 {
            let ptr = std::ptr::addr_of_mut!(self.spu_snd_cap_dad_0);
            unsafe { ptr.add(channel_num * 2).as_mut_unchecked() }
        }

        pub fn spu_snd_cap_len(&mut self, channel_num: usize) -> &mut u16 {
            let ptr = std::ptr::addr_of_mut!(self.spu_snd_cap_len_0);
            unsafe { ptr.add(channel_num * 4).as_mut_unchecked() }
        }
    }
}

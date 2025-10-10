pub mod Arm9Io {
    use crate::core::div_sqrt::DivCnt;
    use crate::core::graphics::gpu::DispCapCnt;
    use crate::core::graphics::gpu::DispStat;
    use crate::core::graphics::gpu_2d::registers_2d::DispCnt;
    use crate::core::graphics::gpu_2d::Gpu2DEngine::{self, A, B};
    use crate::core::graphics::gpu_3d::registers_3d::GxStat;
    use crate::core::graphics::gpu_3d::renderer_3d::Disp3DCnt;
    use crate::core::ipc::{IpcFifoCnt, IpcSyncCnt};
    use crate::core::memory::cartridge::{AuxSpiCnt, RomCtrl};
    use crate::core::timers::TimerCntH;
    use dsvita_macros::io;
    use std::hint::unreachable_unchecked;
    use std::ptr;

    io!(
        (
            io_0! {
                use crate::core::CpuType::ARM9;
                use crate::core::graphics::gpu_2d::Gpu2DEngine::A;
            },
            (io32(0x0, DispCnt), gpu_2d_regs_a_disp_cnt),
            (io16(0x4, DispStat), gpu_disp_stat, || {}, |emu| emu.gpu_set_disp_stat(ARM9)),
            (io16(0x6), gpu_v_count),
            (io16(0x8), gpu_2d_regs_a_bg_cnt_0),
            (io16(0xA), gpu_2d_regs_a_bg_cnt_1),
            (io16(0xC), gpu_2d_regs_a_bg_cnt_2),
            (io16(0xE), gpu_2d_regs_a_bg_cnt_3),
            (io16(0x10), gpu_2d_regs_a_bg_h_ofs_0, || {}, |emu| emu.gpu_2d_regs_set_bg_h_ofs(A, 0)),
            (io16(0x12), gpu_2d_regs_a_bg_v_ofs_0, || {}, |emu| emu.gpu_2d_regs_set_bg_v_ofs(A, 0)),
            (io16(0x14), gpu_2d_regs_a_bg_h_ofs_1, || {}, |emu| emu.gpu_2d_regs_set_bg_h_ofs(A, 1)),
            (io16(0x16), gpu_2d_regs_a_bg_v_ofs_1, || {}, |emu| emu.gpu_2d_regs_set_bg_v_ofs(A, 1)),
            (io16(0x18), gpu_2d_regs_a_bg_h_ofs_2, || {}, |emu| emu.gpu_2d_regs_set_bg_h_ofs(A, 2)),
            (io16(0x1A), gpu_2d_regs_a_bg_v_ofs_2, || {}, |emu| emu.gpu_2d_regs_set_bg_v_ofs(A, 2)),
            (io16(0x1C), gpu_2d_regs_a_bg_h_ofs_3, || {}, |emu| emu.gpu_2d_regs_set_bg_h_ofs(A, 3)),
            (io16(0x1E), gpu_2d_regs_a_bg_v_ofs_3, || {}, |emu| emu.gpu_2d_regs_set_bg_v_ofs(A, 3)),
            (io16(0x20), gpu_2d_regs_a_bg_pa_2),
            (io16(0x22), gpu_2d_regs_a_bg_pb_2),
            (io16(0x24), gpu_2d_regs_a_bg_pc_2),
            (io16(0x26), gpu_2d_regs_a_bg_pd_2),
            (io32(0x28), gpu_2d_regs_a_bg_x_2, || {}, |emu| emu.gpu_2d_regs_set_bg_x(A, 2)),
            (io32(0x2C), gpu_2d_regs_a_bg_y_2, || {}, |emu| emu.gpu_2d_regs_set_bg_y(A, 2)),
            (io16(0x30), gpu_2d_regs_a_bg_pa_3),
            (io16(0x32), gpu_2d_regs_a_bg_pb_3),
            (io16(0x34), gpu_2d_regs_a_bg_pc_3),
            (io16(0x36), gpu_2d_regs_a_bg_pd_3),
            (io32(0x38), gpu_2d_regs_a_bg_x_3, || {}, |emu| emu.gpu_2d_regs_set_bg_x(A, 3)),
            (io32(0x3C), gpu_2d_regs_a_bg_y_3, || {}, |emu| emu.gpu_2d_regs_set_bg_y(A, 3)),
            (io16(0x40), gpu_2d_regs_a_win_h_0),
            (io16(0x42), gpu_2d_regs_a_win_h_1),
            (io16(0x44), gpu_2d_regs_a_win_v_0),
            (io16(0x46), gpu_2d_regs_a_win_v_1),
            (io16(0x48), gpu_2d_regs_a_win_in, || {}, |emu| emu.gpu_2d_regs_set_win_in(A)),
            (io16(0x4A), gpu_2d_regs_a_win_out, || {}, |emu| emu.gpu_2d_regs_set_win_out(A)),
            (io16(0x4C), gpu_2d_regs_a_mosaic),
            (io16(0x50), gpu_2d_regs_a_bld_cnt, || {}, |emu| emu.gpu_2d_regs_set_bld_cnt(A)),
            (io16(0x52), gpu_2d_regs_a_bld_alpha, || {}, |emu| emu.gpu_2d_regs_set_bld_alpha(A)),
            (io8(0x54), gpu_2d_regs_a_bld_y, || {}, |emu| emu.gpu_2d_regs_set_bld_y(A)),
            (io16(0x60, Disp3DCnt), gpu_3d_renderer_disp_cnt, || {}, |emu| emu.gpu_3d_renderer_set_disp_cnt()),
            (io32(0x64, DispCapCnt), gpu_disp_cap_cnt, || {}, |emu| emu.gpu_set_disp_cap_cnt()),
            (io16(0x6C), gpu_2d_regs_a_master_bright, || {}, |emu| emu.gpu_2d_regs_set_master_bright(A)),
            (io32(0xB0), dma_sad_0, || {}, |emu| emu.dma_set_sad(ARM9, 0)),
            (io32(0xB4), dma_dad_0, || {}, |emu| emu.dma_set_dad(ARM9, 0)),
            (io32(0xB8), dma_cnt_0, || {}, |emu| emu.dma_set_cnt(ARM9, 0)),
            (io32(0xBC), dma_sad_1, || {}, |emu| emu.dma_set_sad(ARM9, 1)),
            (io32(0xC0), dma_dad_1, || {}, |emu| emu.dma_set_dad(ARM9, 1)),
            (io32(0xC4), dma_cnt_1, || {}, |emu| emu.dma_set_cnt(ARM9, 1)),
            (io32(0xC8), dma_sad_2, || {}, |emu| emu.dma_set_sad(ARM9, 2)),
            (io32(0xCC), dma_dad_2, || {}, |emu| emu.dma_set_dad(ARM9, 2)),
            (io32(0xD0), dma_cnt_2, || {}, |emu| emu.dma_set_cnt(ARM9, 2)),
            (io32(0xD4), dma_sad_3, || {}, |emu| emu.dma_set_sad(ARM9, 3)),
            (io32(0xD8), dma_dad_3, || {}, |emu| emu.dma_set_dad(ARM9, 3)),
            (io32(0xDC), dma_cnt_3, || {}, |emu| emu.dma_set_cnt(ARM9, 3)),
            (io32(0xE0), dma_fill_0),
            (io32(0xE4), dma_fill_1),
            (io32(0xE8), dma_fill_2),
            (io32(0xEC), dma_fill_3),
        ),
        (
            io_1! {
                use crate::core::CpuType::ARM9;
            },
            (io16(0x100), timers_cnt_l_0, |emu| emu.timers_get_cnt_l(ARM9, 0), |emu| emu.timers_set_cnt_l(ARM9, 0)),
            (io8(0x102, TimerCntH), timers_cnt_h_0, || {}, |emu| emu.timers_set_cnt_h(ARM9, 0)),
            (io16(0x104), timers_cnt_l_1, |emu| emu.timers_get_cnt_l(ARM9, 1), |emu| emu.timers_set_cnt_l(ARM9, 1)),
            (io8(0x106, TimerCntH), timers_cnt_h_1, || {}, |emu| emu.timers_set_cnt_h(ARM9, 1)),
            (io16(0x108), timers_cnt_l_2, |emu| emu.timers_get_cnt_l(ARM9, 2), |emu| emu.timers_set_cnt_l(ARM9, 2)),
            (io8(0x10A, TimerCntH), timers_cnt_h_2, || {}, |emu| emu.timers_set_cnt_h(ARM9, 2)),
            (io16(0x10C), timers_cnt_l_3, |emu| emu.timers_get_cnt_l(ARM9, 3), |emu| emu.timers_set_cnt_l(ARM9, 3)),
            (io8(0x10E, TimerCntH), timers_cnt_h_3, || {}, |emu| emu.timers_set_cnt_h(ARM9, 3)),
            (io16(0x130), input_key, |emu| emu.input.get_key_input()),
            (io16(0x180, IpcSyncCnt), ipc_sync_reg, || {}, |emu| emu.ipc_set_sync_reg(ARM9)),
            (io16(0x184, IpcFifoCnt), ipc_fifo_cnt, |emu| emu.ipc_get_fifo_cnt(ARM9), |emu| emu.ipc_set_fifo_cnt(ARM9)),
            (io32(0x188), ipc_fifo_send, || {}, |emu| emu.ipc_fifo_send(ARM9)),
            (io16(0x1A0, AuxSpiCnt), cartridge_aux_spi_cnt, || {}, |emu| emu.cartridge_set_aux_spi_cnt(ARM9)),
            (io8(0x1A2), cartridge_aux_spi_data, || {}, |emu| emu.cartridge_set_aux_spi_data(ARM9)),
            (io32(0x1A4, RomCtrl), cartridge_rom_ctrl, |emu| emu.cartridge_get_rom_ctrl(ARM9), |emu| emu.cartridge_set_rom_ctrl(ARM9)),
            (io32(0x1A8), cartridge_bus_cmd_out_l),
            (io32(0x1AC), cartridge_bus_cmd_out_h),
        ),
        (
            io_2! {
                use crate::core::CpuType::ARM9;
            },
            (io8(0x208), cpu_ime, || {}, |emu| emu.cpu_set_ime(ARM9)),
            (io32(0x210), cpu_ie, || {}, |emu| emu.cpu_set_ie(ARM9)),
            (io32(0x214), cpu_irf),
            (io8(0x240), vram_cnt_0, || {}, |emu| emu.vram_set_cnt(0)),
            (io8(0x241), vram_cnt_1, || {}, |emu| emu.vram_set_cnt(1)),
            (io8(0x242), vram_cnt_2, || {}, |emu| emu.vram_set_cnt(2)),
            (io8(0x243), vram_cnt_3, || {}, |emu| emu.vram_set_cnt(3)),
            (io8(0x244), vram_cnt_4, || {}, |emu| emu.vram_set_cnt(4)),
            (io8(0x245), vram_cnt_5, || {}, |emu| emu.vram_set_cnt(5)),
            (io8(0x246), vram_cnt_6, || {}, |emu| emu.vram_set_cnt(6)),
            (io8(0x247), wram_cnt, || {}, |emu| emu.wram_set_cnt()),
            (io8(0x248), vram_cnt_7, || {}, |emu| emu.vram_set_cnt(7)),
            (io8(0x249), vram_cnt_8, || {}, |emu| emu.vram_set_cnt(8)),
            (io16(0x280, DivCnt), div_cnt, || {}, |emu| emu.div_set_cnt()),
            (io32(0x290), div_numer_l, || {}, |emu| emu.div_sqrt.set_div_numer_l()),
            (io32(0x294), div_numer_h, || {}, |emu| emu.div_sqrt.set_div_numer_h()),
            (io32(0x298), div_denom_l, || {}, |emu| emu.div_sqrt.set_div_denom_l()),
            (io32(0x29C), div_denom_h, || {}, |emu| emu.div_sqrt.set_div_denom_h()),
            (io32(0x2A0), div_result_l, |emu| emu.div_get_result_l()),
            (io32(0x2A4), div_result_h, |emu| emu.div_get_result_h()),
            (io32(0x2A8), divrem_result_l, |emu| emu.div_get_rem_result_l()),
            (io32(0x2AC), divrem_result_h, |emu| emu.div_get_rem_result_h()),
            (io16(0x2B0), sqrt_cnt, || {}, |emu| emu.sqrt_set_cnt()),
            (io32(0x2B4), sqrt_result, |emu| emu.sqrt_get_result()),
            (io32(0x2B8), sqrt_param_l, || {}, |emu| emu.div_sqrt.set_sqrt_param_l()),
            (io32(0x2BC), sqrt_param_h, || {}, |emu| emu.div_sqrt.set_sqrt_param_h()),
        ),
        (
            io_3! {
                use crate::core::CpuType::ARM9;
            },
            (io8(0x300), cpu_post_flg, || {}, |emu| emu.cpu_set_post_flg(ARM9)),
            (io16(0x304), gpu_pow_cnt1, || {}, |emu| emu.gpu_set_pow_cnt1()),
            (io16(0x330), gpu_3d_renderer_edge_color_0, || {}, |emu| emu.gpu_3d_renderer_set_edge_color(0)),
            (io16(0x332), gpu_3d_renderer_edge_color_1, || {}, |emu| emu.gpu_3d_renderer_set_edge_color(1)),
            (io16(0x334), gpu_3d_renderer_edge_color_2, || {}, |emu| emu.gpu_3d_renderer_set_edge_color(2)),
            (io16(0x336), gpu_3d_renderer_edge_color_3, || {}, |emu| emu.gpu_3d_renderer_set_edge_color(3)),
            (io16(0x338), gpu_3d_renderer_edge_color_4, || {}, |emu| emu.gpu_3d_renderer_set_edge_color(4)),
            (io16(0x33A), gpu_3d_renderer_edge_color_5, || {}, |emu| emu.gpu_3d_renderer_set_edge_color(5)),
            (io16(0x33C), gpu_3d_renderer_edge_color_6, || {}, |emu| emu.gpu_3d_renderer_set_edge_color(6)),
            (io16(0x33E), gpu_3d_renderer_edge_color_7, || {}, |emu| emu.gpu_3d_renderer_set_edge_color(7)),
            (io32(0x350), gpu_3d_renderer_clear_color, || {}, |emu| emu.gpu_3d_renderer_set_clear_color()),
            (io16(0x354), gpu_3d_renderer_clear_depth, || {}, |emu| emu.gpu_3d_renderer_set_clear_depth()),
            (io32(0x358), gpu_3d_renderer_fog_color, || {}, |emu| emu.gpu_3d_renderer_set_fog_color()),
            (io16(0x35C), gpu_3d_renderer_fog_offset, || {}, |emu| emu.gpu_3d_renderer_set_fog_offset()),
            (io8(0x360), gpu_3d_renderer_fog_table_0, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(0)),
            (io8(0x361), gpu_3d_renderer_fog_table_1, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(1)),
            (io8(0x362), gpu_3d_renderer_fog_table_2, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(2)),
            (io8(0x363), gpu_3d_renderer_fog_table_3, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(3)),
            (io8(0x364), gpu_3d_renderer_fog_table_4, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(4)),
            (io8(0x365), gpu_3d_renderer_fog_table_5, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(5)),
            (io8(0x366), gpu_3d_renderer_fog_table_6, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(6)),
            (io8(0x367), gpu_3d_renderer_fog_table_7, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(7)),
            (io8(0x368), gpu_3d_renderer_fog_table_8, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(8)),
            (io8(0x369), gpu_3d_renderer_fog_table_9, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(9)),
            (io8(0x36A), gpu_3d_renderer_fog_table_10, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(10)),
            (io8(0x36B), gpu_3d_renderer_fog_table_11, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(11)),
            (io8(0x36C), gpu_3d_renderer_fog_table_12, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(12)),
            (io8(0x36D), gpu_3d_renderer_fog_table_13, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(13)),
            (io8(0x36E), gpu_3d_renderer_fog_table_14, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(14)),
            (io8(0x36F), gpu_3d_renderer_fog_table_15, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(15)),
            (io8(0x370), gpu_3d_renderer_fog_table_16, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(16)),
            (io8(0x371), gpu_3d_renderer_fog_table_17, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(17)),
            (io8(0x372), gpu_3d_renderer_fog_table_18, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(18)),
            (io8(0x373), gpu_3d_renderer_fog_table_19, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(19)),
            (io8(0x374), gpu_3d_renderer_fog_table_20, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(20)),
            (io8(0x375), gpu_3d_renderer_fog_table_21, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(21)),
            (io8(0x376), gpu_3d_renderer_fog_table_22, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(22)),
            (io8(0x377), gpu_3d_renderer_fog_table_23, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(23)),
            (io8(0x378), gpu_3d_renderer_fog_table_24, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(24)),
            (io8(0x379), gpu_3d_renderer_fog_table_25, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(25)),
            (io8(0x37A), gpu_3d_renderer_fog_table_26, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(26)),
            (io8(0x37B), gpu_3d_renderer_fog_table_27, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(27)),
            (io8(0x37C), gpu_3d_renderer_fog_table_28, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(28)),
            (io8(0x37D), gpu_3d_renderer_fog_table_29, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(29)),
            (io8(0x37E), gpu_3d_renderer_fog_table_30, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(30)),
            (io8(0x37F), gpu_3d_renderer_fog_table_31, || {}, |emu| emu.gpu_3d_renderer_set_fog_table(31)),
            (io16(0x380), gpu_3d_renderer_toon_table_0, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(0)),
            (io16(0x382), gpu_3d_renderer_toon_table_1, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(1)),
            (io16(0x384), gpu_3d_renderer_toon_table_2, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(2)),
            (io16(0x386), gpu_3d_renderer_toon_table_3, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(3)),
            (io16(0x388), gpu_3d_renderer_toon_table_4, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(4)),
            (io16(0x38A), gpu_3d_renderer_toon_table_5, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(5)),
            (io16(0x38C), gpu_3d_renderer_toon_table_6, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(6)),
            (io16(0x38E), gpu_3d_renderer_toon_table_7, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(7)),
            (io16(0x390), gpu_3d_renderer_toon_table_8, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(8)),
            (io16(0x392), gpu_3d_renderer_toon_table_9, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(9)),
            (io16(0x394), gpu_3d_renderer_toon_table_10, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(10)),
            (io16(0x396), gpu_3d_renderer_toon_table_11, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(11)),
            (io16(0x398), gpu_3d_renderer_toon_table_12, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(12)),
            (io16(0x39A), gpu_3d_renderer_toon_table_13, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(13)),
            (io16(0x39C), gpu_3d_renderer_toon_table_14, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(14)),
            (io16(0x39E), gpu_3d_renderer_toon_table_15, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(15)),
            (io16(0x3A0), gpu_3d_renderer_toon_table_16, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(16)),
            (io16(0x3A2), gpu_3d_renderer_toon_table_17, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(17)),
            (io16(0x3A4), gpu_3d_renderer_toon_table_18, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(18)),
            (io16(0x3A6), gpu_3d_renderer_toon_table_19, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(19)),
            (io16(0x3A8), gpu_3d_renderer_toon_table_20, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(20)),
            (io16(0x3AA), gpu_3d_renderer_toon_table_21, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(21)),
            (io16(0x3AC), gpu_3d_renderer_toon_table_22, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(22)),
            (io16(0x3AE), gpu_3d_renderer_toon_table_23, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(23)),
            (io16(0x3B0), gpu_3d_renderer_toon_table_24, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(24)),
            (io16(0x3B2), gpu_3d_renderer_toon_table_25, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(25)),
            (io16(0x3B4), gpu_3d_renderer_toon_table_26, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(26)),
            (io16(0x3B6), gpu_3d_renderer_toon_table_27, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(27)),
            (io16(0x3B8), gpu_3d_renderer_toon_table_28, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(28)),
            (io16(0x3BA), gpu_3d_renderer_toon_table_29, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(29)),
            (io16(0x3BC), gpu_3d_renderer_toon_table_30, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(30)),
            (io16(0x3BE), gpu_3d_renderer_toon_table_31, || {}, |emu| emu.gpu_3d_renderer_set_toon_table(31)),
        ),
        (
            io_4! {},
            (io32(0x400), gpu_3d_regs_gx_fifo_0, || {}, |emu| emu.regs_3d_set_io_gx_fifo(0)),
            (io32(0x404), gpu_3d_regs_gx_fifo_1, || {}, |emu| emu.regs_3d_set_io_gx_fifo(1)),
            (io32(0x408), gpu_3d_regs_gx_fifo_2, || {}, |emu| emu.regs_3d_set_io_gx_fifo(2)),
            (io32(0x40C), gpu_3d_regs_gx_fifo_3, || {}, |emu| emu.regs_3d_set_io_gx_fifo(3)),
            (io32(0x410), gpu_3d_regs_gx_fifo_4, || {}, |emu| emu.regs_3d_set_io_gx_fifo(4)),
            (io32(0x414), gpu_3d_regs_gx_fifo_5, || {}, |emu| emu.regs_3d_set_io_gx_fifo(5)),
            (io32(0x418), gpu_3d_regs_gx_fifo_6, || {}, |emu| emu.regs_3d_set_io_gx_fifo(6)),
            (io32(0x41C), gpu_3d_regs_gx_fifo_7, || {}, |emu| emu.regs_3d_set_io_gx_fifo(7)),
            (io32(0x420), gpu_3d_regs_gx_fifo_8, || {}, |emu| emu.regs_3d_set_io_gx_fifo(8)),
            (io32(0x424), gpu_3d_regs_gx_fifo_9, || {}, |emu| emu.regs_3d_set_io_gx_fifo(9)),
            (io32(0x428), gpu_3d_regs_gx_fifo_10, || {}, |emu| emu.regs_3d_set_io_gx_fifo(10)),
            (io32(0x42C), gpu_3d_regs_gx_fifo_11, || {}, |emu| emu.regs_3d_set_io_gx_fifo(11)),
            (io32(0x430), gpu_3d_regs_gx_fifo_12, || {}, |emu| emu.regs_3d_set_io_gx_fifo(12)),
            (io32(0x434), gpu_3d_regs_gx_fifo_13, || {}, |emu| emu.regs_3d_set_io_gx_fifo(13)),
            (io32(0x438), gpu_3d_regs_gx_fifo_14, || {}, |emu| emu.regs_3d_set_io_gx_fifo(14)),
            (io32(0x43C), gpu_3d_regs_gx_fifo_15, || {}, |emu| emu.regs_3d_set_io_gx_fifo(15)),
            (io32(0x440), gpu_3d_regs_mtx_mode, || {}, |emu| emu.regs_3d_set_mtx_mode()),
            (io32(0x444), gpu_3d_regs_mtx_push, || {}, |emu| emu.regs_3d_set_mtx_push()),
            (io32(0x448), gpu_3d_regs_mtx_pop, || {}, |emu| emu.regs_3d_set_mtx_pop()),
            (io32(0x44C), gpu_3d_regs_mtx_store, || {}, |emu| emu.regs_3d_set_mtx_store()),
            (io32(0x450), gpu_3d_regs_mtx_restore, || {}, |emu| emu.regs_3d_set_mtx_restore()),
            (io32(0x454), gpu_3d_regs_mtx_identity, || {}, |emu| emu.regs_3d_set_mtx_identity()),
            (io32(0x458), gpu_3d_regs_mtx_load44, || {}, |emu| emu.regs_3d_set_mtx_load44()),
            (io32(0x45C), gpu_3d_regs_mtx_load43, || {}, |emu| emu.regs_3d_set_mtx_load43()),
            (io32(0x460), gpu_3d_regs_mtx_mult44, || {}, |emu| emu.regs_3d_set_mtx_mult44()),
            (io32(0x464), gpu_3d_regs_mtx_mult43, || {}, |emu| emu.regs_3d_set_mtx_mult43()),
            (io32(0x468), gpu_3d_regs_mtx_mult33, || {}, |emu| emu.regs_3d_set_mtx_mult33()),
            (io32(0x46C), gpu_3d_regs_mtx_scale, || {}, |emu| emu.regs_3d_set_mtx_scale()),
            (io32(0x470), gpu_3d_regs_mtx_trans, || {}, |emu| emu.regs_3d_set_mtx_trans()),
            (io32(0x480), gpu_3d_regs_color, || {}, |emu| emu.regs_3d_set_color()),
            (io32(0x484), gpu_3d_regs_normal, || {}, |emu| emu.regs_3d_set_normal()),
            (io32(0x488), gpu_3d_regs_tex_coord, || {}, |emu| emu.regs_3d_set_tex_coord()),
            (io32(0x48C), gpu_3d_regs_vtx16, || {}, |emu| emu.regs_3d_set_vtx16()),
            (io32(0x490), gpu_3d_regs_vtx10, || {}, |emu| emu.regs_3d_set_vtx10()),
            (io32(0x494), gpu_3d_regs_vtx_x_y, || {}, |emu| emu.regs_3d_set_vtx_x_y()),
            (io32(0x498), gpu_3d_regs_vtx_x_z, || {}, |emu| emu.regs_3d_set_vtx_x_z()),
            (io32(0x49C), gpu_3d_regs_vtx_y_z, || {}, |emu| emu.regs_3d_set_vtx_y_z()),
            (io32(0x4A0), gpu_3d_regs_vtx_diff, || {}, |emu| emu.regs_3d_set_vtx_diff()),
            (io32(0x4A4), gpu_3d_regs_polygon_attr, || {}, |emu| emu.regs_3d_set_polygon_attr()),
            (io32(0x4A8), gpu_3d_regs_tex_image_param, || {}, |emu| emu.regs_3d_set_tex_image_param()),
            (io32(0x4AC), gpu_3d_regs_pltt_base, || {}, |emu| emu.regs_3d_set_pltt_base()),
            (io32(0x4C0), gpu_3d_regs_dif_amb, || {}, |emu| emu.regs_3d_set_dif_amb()),
            (io32(0x4C4), gpu_3d_regs_spe_emi, || {}, |emu| emu.regs_3d_set_spe_emi()),
            (io32(0x4C8), gpu_3d_regs_light_vector, || {}, |emu| emu.regs_3d_set_light_vector()),
            (io32(0x4CC), gpu_3d_regs_light_color, || {}, |emu| emu.regs_3d_set_light_color()),
            (io32(0x4D0), gpu_3d_regs_shininess, || {}, |emu| emu.regs_3d_set_shininess()),
        ),
        (
            io_5! {},
            (io32(0x500), gpu_3d_regs_begin_vtxs, || {}, |emu| emu.regs_3d_set_begin_vtxs()),
            (io32(0x504), gpu_3d_regs_end_vtxs, || {}, |emu| emu.regs_3d_set_end_vtxs()),
            (io32(0x540), gpu_3d_regs_swap_buffers, || {}, |emu| emu.regs_3d_set_swap_buffers()),
            (io32(0x580), gpu_3d_regs_viewport, || {}, |emu| emu.regs_3d_set_viewport()),
            (io32(0x5C0), gpu_3d_regs_box_test, || {}, |emu| emu.regs_3d_set_box_test()),
            (io32(0x5C4), gpu_3d_regs_pos_test, || {}, |emu| emu.regs_3d_set_pos_test()),
            (io32(0x5C8), gpu_3d_regs_vec_test, || {}, |emu| emu.regs_3d_set_vec_test()),
        ),
        (
            io_6! {},
            (io32(0x600, GxStat), gpu_3d_regs_gx_stat, |emu| emu.regs_3d_get_gx_stat()),
            (io32(0x604), gpu_3d_regs_ram_count, |emu| emu.regs_3d_get_ram_count()),
            (io32(0x620), gpu_3d_regs_pos_result_0, |emu| emu.regs_3d_get_pos_result(0)),
            (io32(0x624), gpu_3d_regs_pos_result_1, |emu| emu.regs_3d_get_pos_result(1)),
            (io32(0x628), gpu_3d_regs_pos_result_2, |emu| emu.regs_3d_get_pos_result(2)),
            (io32(0x62C), gpu_3d_regs_pos_result_3, |emu| emu.regs_3d_get_pos_result(3)),
            (io16(0x630), gpu_3d_regs_vec_result_0, |emu| emu.regs_3d_get_vec_result(0)),
            (io16(0x632), gpu_3d_regs_vec_result_1, |emu| emu.regs_3d_get_vec_result(1)),
            (io16(0x634), gpu_3d_regs_vec_result_2, |emu| emu.regs_3d_get_vec_result(2)),
            (io32(0x640), gpu_3d_regs_clip_mtx_result_0, |emu| emu.regs_3d_get_clip_mtx_result(0)),
            (io32(0x644), gpu_3d_regs_clip_mtx_result_1, |emu| emu.regs_3d_get_clip_mtx_result(1)),
            (io32(0x648), gpu_3d_regs_clip_mtx_result_2, |emu| emu.regs_3d_get_clip_mtx_result(2)),
            (io32(0x64C), gpu_3d_regs_clip_mtx_result_3, |emu| emu.regs_3d_get_clip_mtx_result(3)),
            (io32(0x650), gpu_3d_regs_clip_mtx_result_4, |emu| emu.regs_3d_get_clip_mtx_result(4)),
            (io32(0x654), gpu_3d_regs_clip_mtx_result_5, |emu| emu.regs_3d_get_clip_mtx_result(5)),
            (io32(0x658), gpu_3d_regs_clip_mtx_result_6, |emu| emu.regs_3d_get_clip_mtx_result(6)),
            (io32(0x65C), gpu_3d_regs_clip_mtx_result_7, |emu| emu.regs_3d_get_clip_mtx_result(7)),
            (io32(0x660), gpu_3d_regs_clip_mtx_result_8, |emu| emu.regs_3d_get_clip_mtx_result(8)),
            (io32(0x664), gpu_3d_regs_clip_mtx_result_9, |emu| emu.regs_3d_get_clip_mtx_result(9)),
            (io32(0x668), gpu_3d_regs_clip_mtx_result_10, |emu| emu.regs_3d_get_clip_mtx_result(10)),
            (io32(0x66C), gpu_3d_regs_clip_mtx_result_11, |emu| emu.regs_3d_get_clip_mtx_result(11)),
            (io32(0x670), gpu_3d_regs_clip_mtx_result_12, |emu| emu.regs_3d_get_clip_mtx_result(12)),
            (io32(0x674), gpu_3d_regs_clip_mtx_result_13, |emu| emu.regs_3d_get_clip_mtx_result(13)),
            (io32(0x678), gpu_3d_regs_clip_mtx_result_14, |emu| emu.regs_3d_get_clip_mtx_result(14)),
            (io32(0x67C), gpu_3d_regs_clip_mtx_result_15, |emu| emu.regs_3d_get_clip_mtx_result(15)),
            (io32(0x680), gpu_3d_regs_vec_mtx_result_0, |emu| emu.regs_3d_get_vec_mtx_result(0)),
            (io32(0x684), gpu_3d_regs_vec_mtx_result_1, |emu| emu.regs_3d_get_vec_mtx_result(1)),
            (io32(0x688), gpu_3d_regs_vec_mtx_result_2, |emu| emu.regs_3d_get_vec_mtx_result(2)),
            (io32(0x68C), gpu_3d_regs_vec_mtx_result_3, |emu| emu.regs_3d_get_vec_mtx_result(3)),
            (io32(0x690), gpu_3d_regs_vec_mtx_result_4, |emu| emu.regs_3d_get_vec_mtx_result(4)),
            (io32(0x694), gpu_3d_regs_vec_mtx_result_5, |emu| emu.regs_3d_get_vec_mtx_result(5)),
            (io32(0x698), gpu_3d_regs_vec_mtx_result_6, |emu| emu.regs_3d_get_vec_mtx_result(6)),
            (io32(0x69C), gpu_3d_regs_vec_mtx_result_7, |emu| emu.regs_3d_get_vec_mtx_result(7)),
            (io32(0x6A0), gpu_3d_regs_vec_mtx_result_8, |emu| emu.regs_3d_get_vec_mtx_result(8)),
        ),
        (
            io_gpu_b! {
                use crate::core::graphics::gpu_2d::Gpu2DEngine::B;
            },
            (io32(0x1000, DispCnt), gpu_2d_regs_b_disp_cnt, || {}, |emu| emu.gpu_2d_regs_b_set_disp_cnt()),
            (io16(0x1008), gpu_2d_regs_b_bg_cnt_0),
            (io16(0x100A), gpu_2d_regs_b_bg_cnt_1),
            (io16(0x100C), gpu_2d_regs_b_bg_cnt_2),
            (io16(0x100E), gpu_2d_regs_b_bg_cnt_3),
            (io16(0x1010), gpu_2d_regs_b_bg_h_ofs_0, || {}, |emu| emu.gpu_2d_regs_set_bg_h_ofs(B, 0)),
            (io16(0x1012), gpu_2d_regs_b_bg_v_ofs_0, || {}, |emu| emu.gpu_2d_regs_set_bg_v_ofs(B, 0)),
            (io16(0x1014), gpu_2d_regs_b_bg_h_ofs_1, || {}, |emu| emu.gpu_2d_regs_set_bg_h_ofs(B, 1)),
            (io16(0x1016), gpu_2d_regs_b_bg_v_ofs_1, || {}, |emu| emu.gpu_2d_regs_set_bg_v_ofs(B, 1)),
            (io16(0x1018), gpu_2d_regs_b_bg_h_ofs_2, || {}, |emu| emu.gpu_2d_regs_set_bg_h_ofs(B, 2)),
            (io16(0x101A), gpu_2d_regs_b_bg_v_ofs_2, || {}, |emu| emu.gpu_2d_regs_set_bg_v_ofs(B, 2)),
            (io16(0x101C), gpu_2d_regs_b_bg_h_ofs_3, || {}, |emu| emu.gpu_2d_regs_set_bg_h_ofs(B, 3)),
            (io16(0x101E), gpu_2d_regs_b_bg_v_ofs_3, || {}, |emu| emu.gpu_2d_regs_set_bg_v_ofs(B, 3)),
            (io16(0x1020), gpu_2d_regs_b_bg_pa_2),
            (io16(0x1022), gpu_2d_regs_b_bg_pb_2),
            (io16(0x1024), gpu_2d_regs_b_bg_pc_2),
            (io16(0x1026), gpu_2d_regs_b_bg_pd_2),
            (io32(0x1028), gpu_2d_regs_b_bg_x_2, || {}, |emu| emu.gpu_2d_regs_set_bg_x(B, 2)),
            (io32(0x102C), gpu_2d_regs_b_bg_y_2, || {}, |emu| emu.gpu_2d_regs_set_bg_y(B, 2)),
            (io16(0x1030), gpu_2d_regs_b_bg_pa_3),
            (io16(0x1032), gpu_2d_regs_b_bg_pb_3),
            (io16(0x1034), gpu_2d_regs_b_bg_pc_3),
            (io16(0x1036), gpu_2d_regs_b_bg_pd_3),
            (io32(0x1038), gpu_2d_regs_b_bg_x_3, || {}, |emu| emu.gpu_2d_regs_set_bg_x(B, 3)),
            (io32(0x103C), gpu_2d_regs_b_bg_y_3, || {}, |emu| emu.gpu_2d_regs_set_bg_y(B, 3)),
            (io16(0x1040), gpu_2d_regs_b_win_h_0),
            (io16(0x1042), gpu_2d_regs_b_win_h_1),
            (io16(0x1044), gpu_2d_regs_b_win_v_0),
            (io16(0x1046), gpu_2d_regs_b_win_v_1),
            (io16(0x1048), gpu_2d_regs_b_win_in, || {}, |emu| emu.gpu_2d_regs_set_win_in(B)),
            (io16(0x104A), gpu_2d_regs_b_win_out, || {}, |emu| emu.gpu_2d_regs_set_win_out(B)),
            (io16(0x104C), gpu_2d_regs_b_mosaic),
            (io16(0x1050), gpu_2d_regs_b_bld_cnt, || {}, |emu| emu.gpu_2d_regs_set_bld_cnt(B)),
            (io16(0x1052), gpu_2d_regs_b_bld_alpha, || {}, |emu| emu.gpu_2d_regs_set_bld_alpha(B)),
            (io8(0x1054), gpu_2d_regs_b_bld_y, || {}, |emu| emu.gpu_2d_regs_set_bld_y(B)),
            (io16(0x106C), gpu_2d_regs_b_master_bright, || {}, |emu| emu.gpu_2d_regs_set_master_bright(B)),
        ),
        (
            io_upper! {
                use crate::core::CpuType::ARM9;
            },
            (io32(0x100000), ipc_fifo_recv, |emu| emu.ipc_fifo_recv(ARM9)),
            (io32(0x100010), cartridge_rom_data_in, |emu| emu.cartridge_get_rom_data_in(ARM9)),
        ),
    );

    impl Memory {
        pub fn gpu_2d_regs_disp_cnt(&mut self, engine: Gpu2DEngine) -> &mut DispCnt {
            match engine {
                A => &mut self.gpu_2d_regs_a_disp_cnt,
                B => &mut self.gpu_2d_regs_b_disp_cnt,
            }
        }

        pub fn gpu_2d_regs_bg_cnt(&mut self, engine: Gpu2DEngine, bg_num: usize) -> &mut u16 {
            let ptr = match engine {
                A => std::ptr::addr_of_mut!(self.gpu_2d_regs_a_bg_cnt_0),
                B => std::ptr::addr_of_mut!(self.gpu_2d_regs_b_bg_cnt_0),
            };
            unsafe { ptr.add(bg_num).as_mut_unchecked() }
        }

        pub fn gpu_2d_regs_bg_h_ofs(&mut self, engine: Gpu2DEngine, bg_num: usize) -> &mut u16 {
            let ptr = match engine {
                A => std::ptr::addr_of_mut!(self.gpu_2d_regs_a_bg_h_ofs_0),
                B => std::ptr::addr_of_mut!(self.gpu_2d_regs_b_bg_h_ofs_0),
            };
            unsafe { ptr.add(bg_num * 2).as_mut_unchecked() }
        }

        pub fn gpu_2d_regs_bg_v_ofs(&mut self, engine: Gpu2DEngine, bg_num: usize) -> &mut u16 {
            let ptr = match engine {
                A => std::ptr::addr_of_mut!(self.gpu_2d_regs_a_bg_v_ofs_0),
                B => std::ptr::addr_of_mut!(self.gpu_2d_regs_b_bg_v_ofs_0),
            };
            unsafe { ptr.add(bg_num * 2).as_mut_unchecked() }
        }

        pub fn gpu_2d_regs_bg_x(&mut self, engine: Gpu2DEngine, bg_num: usize) -> &mut u32 {
            match (engine, bg_num) {
                (A, 2) => &mut self.gpu_2d_regs_a_bg_x_2,
                (A, 3) => &mut self.gpu_2d_regs_a_bg_x_3,
                (B, 2) => &mut self.gpu_2d_regs_b_bg_x_2,
                (B, 3) => &mut self.gpu_2d_regs_b_bg_x_3,
                _ => unsafe { unreachable_unchecked() },
            }
        }

        pub fn gpu_2d_regs_bg_y(&mut self, engine: Gpu2DEngine, bg_num: usize) -> &mut u32 {
            match (engine, bg_num) {
                (A, 2) => &mut self.gpu_2d_regs_a_bg_y_2,
                (A, 3) => &mut self.gpu_2d_regs_a_bg_y_3,
                (B, 2) => &mut self.gpu_2d_regs_b_bg_y_2,
                (B, 3) => &mut self.gpu_2d_regs_b_bg_y_3,
                _ => unsafe { unreachable_unchecked() },
            }
        }

        pub fn gpu_2d_regs_bg_pa(&mut self, engine: Gpu2DEngine, bg_num: usize) -> &mut u16 {
            match (engine, bg_num) {
                (A, 2) => &mut self.gpu_2d_regs_a_bg_pa_2,
                (A, 3) => &mut self.gpu_2d_regs_a_bg_pa_3,
                (B, 2) => &mut self.gpu_2d_regs_b_bg_pa_2,
                (B, 3) => &mut self.gpu_2d_regs_b_bg_pa_3,
                _ => unsafe { unreachable_unchecked() },
            }
        }

        pub fn gpu_2d_regs_bg_pb(&mut self, engine: Gpu2DEngine, bg_num: usize) -> &mut u16 {
            match (engine, bg_num) {
                (A, 2) => &mut self.gpu_2d_regs_a_bg_pb_2,
                (A, 3) => &mut self.gpu_2d_regs_a_bg_pb_3,
                (B, 2) => &mut self.gpu_2d_regs_b_bg_pb_2,
                (B, 3) => &mut self.gpu_2d_regs_b_bg_pb_3,
                _ => unsafe { unreachable_unchecked() },
            }
        }

        pub fn gpu_2d_regs_bg_pc(&mut self, engine: Gpu2DEngine, bg_num: usize) -> &mut u16 {
            match (engine, bg_num) {
                (A, 2) => &mut self.gpu_2d_regs_a_bg_pc_2,
                (A, 3) => &mut self.gpu_2d_regs_a_bg_pc_3,
                (B, 2) => &mut self.gpu_2d_regs_b_bg_pc_2,
                (B, 3) => &mut self.gpu_2d_regs_b_bg_pc_3,
                _ => unsafe { unreachable_unchecked() },
            }
        }

        pub fn gpu_2d_regs_bg_pd(&mut self, engine: Gpu2DEngine, bg_num: usize) -> &mut u16 {
            match (engine, bg_num) {
                (A, 2) => &mut self.gpu_2d_regs_a_bg_pd_2,
                (A, 3) => &mut self.gpu_2d_regs_a_bg_pd_3,
                (B, 2) => &mut self.gpu_2d_regs_b_bg_pd_2,
                (B, 3) => &mut self.gpu_2d_regs_b_bg_pd_3,
                _ => unsafe { unreachable_unchecked() },
            }
        }

        pub fn gpu_2d_regs_win_v(&mut self, engine: Gpu2DEngine, win: usize) -> &mut u16 {
            let ptr = match engine {
                A => std::ptr::addr_of_mut!(self.gpu_2d_regs_a_win_v_0),
                B => std::ptr::addr_of_mut!(self.gpu_2d_regs_b_win_v_0),
            };
            unsafe { ptr.add(win).as_mut_unchecked() }
        }

        pub fn gpu_2d_regs_win_h(&mut self, engine: Gpu2DEngine, win: usize) -> &mut u16 {
            let ptr = match engine {
                A => std::ptr::addr_of_mut!(self.gpu_2d_regs_a_win_h_0),
                B => std::ptr::addr_of_mut!(self.gpu_2d_regs_b_win_h_0),
            };
            unsafe { ptr.add(win).as_mut_unchecked() }
        }

        pub fn gpu_2d_regs_win_in(&mut self, engine: Gpu2DEngine) -> &mut u16 {
            match engine {
                A => &mut self.gpu_2d_regs_a_win_in,
                B => &mut self.gpu_2d_regs_b_win_in,
            }
        }

        pub fn gpu_2d_regs_win_out(&mut self, engine: Gpu2DEngine) -> &mut u16 {
            match engine {
                A => &mut self.gpu_2d_regs_a_win_out,
                B => &mut self.gpu_2d_regs_b_win_out,
            }
        }

        pub fn gpu_2d_regs_bld_cnt(&mut self, engine: Gpu2DEngine) -> &mut u16 {
            match engine {
                A => &mut self.gpu_2d_regs_a_bld_cnt,
                B => &mut self.gpu_2d_regs_b_bld_cnt,
            }
        }

        pub fn gpu_2d_regs_bld_alpha(&mut self, engine: Gpu2DEngine) -> &mut u16 {
            match engine {
                A => &mut self.gpu_2d_regs_a_bld_alpha,
                B => &mut self.gpu_2d_regs_b_bld_alpha,
            }
        }

        pub fn gpu_2d_regs_bld_y(&mut self, engine: Gpu2DEngine) -> &mut u8 {
            match engine {
                A => &mut self.gpu_2d_regs_a_bld_y,
                B => &mut self.gpu_2d_regs_b_bld_y,
            }
        }

        pub fn gpu_2d_regs_master_bright(&mut self, engine: Gpu2DEngine) -> &mut u16 {
            match engine {
                A => &mut self.gpu_2d_regs_a_master_bright,
                B => &mut self.gpu_2d_regs_b_master_bright,
            }
        }

        pub fn vram_cnt(&mut self, bank: usize) -> &mut u8 {
            let ptr = ptr::addr_of_mut!(self.vram_cnt_0);
            unsafe { ptr.add(bank).as_mut_unchecked() }
        }

        pub fn gpu_3d_renderer_edge_color(&mut self, index: usize) -> &mut u16 {
            let ptr = ptr::addr_of_mut!(self.gpu_3d_renderer_edge_color_0);
            unsafe { ptr.add(index).as_mut_unchecked() }
        }

        pub fn gpu_3d_renderer_fog_table(&mut self, index: usize) -> &mut u8 {
            let ptr = ptr::addr_of_mut!(self.gpu_3d_renderer_fog_table_0);
            unsafe { ptr.add(index).as_mut_unchecked() }
        }

        pub fn gpu_3d_renderer_toon_table(&mut self, index: usize) -> &mut u16 {
            let ptr = ptr::addr_of_mut!(self.gpu_3d_renderer_toon_table_0);
            unsafe { ptr.add(index).as_mut_unchecked() }
        }

        pub fn gpu_3d_regs_gx_fifo(&mut self, index: usize) -> &mut u32 {
            let ptr = ptr::addr_of_mut!(self.gpu_3d_regs_gx_fifo_0);
            unsafe { ptr.add(index).as_mut_unchecked() }
        }

        pub fn gpu_3d_regs_pos_result(&mut self, index: usize) -> &mut u32 {
            let ptr = ptr::addr_of_mut!(self.gpu_3d_regs_pos_result_0);
            unsafe { ptr.add(index).as_mut_unchecked() }
        }

        pub fn gpu_3d_regs_vec_result(&mut self, index: usize) -> &mut u16 {
            let ptr = ptr::addr_of_mut!(self.gpu_3d_regs_vec_result_0);
            unsafe { ptr.add(index).as_mut_unchecked() }
        }

        pub fn gpu_3d_regs_clip_mtx_result(&mut self, index: usize) -> &mut u32 {
            let ptr = ptr::addr_of_mut!(self.gpu_3d_regs_clip_mtx_result_0);
            unsafe { ptr.add(index).as_mut_unchecked() }
        }

        pub fn gpu_3d_regs_vec_mtx_result(&mut self, index: usize) -> &mut u32 {
            let ptr = ptr::addr_of_mut!(self.gpu_3d_regs_vec_mtx_result_0);
            unsafe { ptr.add(index).as_mut_unchecked() }
        }
    }
}

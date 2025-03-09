use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::{get_cm_mut, get_common, get_common_mut, get_cpu_regs_mut, get_mem_mut, io_dma, Emu};
use crate::core::graphics::gpu_3d::geometry_3d::Gpu3DGeometry;
use crate::core::memory::dma::DmaTransferMode;
use crate::core::CpuType::ARM9;
use bilge::prelude::*;
use paste::paste;
use std::cmp::max;
use std::intrinsics::unlikely;
use std::mem;
use std::ops::DerefMut;
use std::sync::atomic::Ordering;

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
pub struct GxStat {
    pub box_pos_vec_test_busy: bool,
    pub box_test_result: bool,
    not_used: u6,
    pub pos_vec_mtx_stack_lvl: u5,
    pub proj_mtx_stack_lvl: u1,
    pub mtx_stack_busy: bool,
    pub mtx_stack_overflow_underflow_err: bool,
    pub num_entries_cmd_fifo: u9,
    pub cmd_fifo_less_half_full: bool,
    pub cmd_fifo_empty: bool,
    pub geometry_busy: bool,
    not_used2: u2,
    pub cmd_fifo_irq: u2,
}

impl Default for GxStat {
    fn default() -> Self {
        0x04000000.into()
    }
}

pub const FIFO_PARAM_COUNTS: [u8; 128] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x00-0x0F
    1, 0, 1, 1, 1, 0, 16, 12, 16, 12, 9, 3, 3, 0, 0, 0, // 0x10-0x1F
    1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, // 0x20-0x2F
    1, 1, 1, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x30-0x3F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x40-0x4F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x50-0x5F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x60-0x6F
    3, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x70-0x7F
];

pub const FUNC_NAME_LUT: [&str; 128] = [
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_mtx_mode",
    "exe_mtx_push",
    "exe_mtx_pop",
    "exe_mtx_store",
    "exe_mtx_restore",
    "exe_mtx_identity",
    "exe_mtx_load44",
    "exe_mtx_load43",
    "exe_mtx_mult44",
    "exe_mtx_mult43",
    "exe_mtx_mult33",
    "exe_mtx_scale",
    "exe_mtx_trans",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_color",
    "exe_normal",
    "exe_tex_coord",
    "exe_vtx16",
    "exe_vtx10",
    "exe_vtx_x_y",
    "exe_vtx_x_z",
    "exe_vtx_y_z",
    "exe_vtx_diff",
    "exe_polygon_attr",
    "exe_tex_image_param",
    "exe_pltt_base",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_dif_amb",
    "exe_spe_emi",
    "exe_light_vector",
    "exe_light_color",
    "exe_shininess",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_begin_vtxs",
    "exe_end_vtxs",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_swap_buffers",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_viewport",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_box_test",
    "exe_pos_test",
    "exe_vec_test",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
];

#[derive(Default)]
pub struct Gpu3DRegisters {
    cmd_fifo: Vec<u32>,
    cmd_remaining_params: u8,
    last_cmd: u32,
    cmd_fifo_len: u16,
    processing_offset: usize,

    mtx_push_pop_queue: u32,
    test_queue: u8,

    last_total_cycles: u64,
    pub flushed: bool,

    gx_stat: GxStat,
    acknowledge_error: bool,

    vertices_size: u16,
    polygons_size: u16,
}

macro_rules! unpacked_cmd {
    ($name:ident, $cmd:expr) => {
        paste! {
            pub fn [<set _ $name>](&mut self, mask: u32, value: u32, emu: &mut Emu) {
                self.queue_unpacked_value::<$cmd>(value & mask, emu);
            }
        }
    };
}

impl Gpu3DRegisters {
    fn is_cmd_fifo_full(&self) -> bool {
        self.cmd_fifo_len >= 260
    }

    pub fn is_cmd_fifo_half_full(&self) -> bool {
        self.cmd_fifo_len >= 132
    }

    fn is_cmd_fifo_empty(&self) -> bool {
        self.cmd_fifo_len <= 4
    }

    fn get_cmd_fifo_len(&self) -> usize {
        max(self.cmd_fifo_len as isize - 4, 0) as usize
    }

    fn can_run_cmds(&self) -> bool {
        self.processing_offset < self.cmd_fifo.len() && {
            let params_count = self.cmd_fifo[self.processing_offset] >> 8;
            (params_count as usize) < self.cmd_fifo.len() - self.processing_offset
        }
    }

    pub fn run_cmds(&mut self, total_cycles: u64, emu: &mut Emu) {
        if self.flushed || !self.can_run_cmds() {
            self.last_total_cycles = total_cycles;
            let geometry = get_common_mut!(emu).gpu.get_3d_geometry_mut();
            if !geometry.processing.load(Ordering::Acquire) {
                if self.processing_offset != 0 {
                    self.queue_geometry(emu);
                } else if geometry.needs_sync {
                    self.geometry_sync(geometry);
                }
                // println!("geometry sync return");
            }
            return;
        }

        let is_cmd_fifo_half_full = self.is_cmd_fifo_half_full();

        let cycle_diff = (total_cycles - self.last_total_cycles) as u32;
        self.last_total_cycles = total_cycles;
        let mut executed_cycles = 0;

        let mut can_run_cmds;
        while {
            let value = self.cmd_fifo[self.processing_offset] as usize;
            self.processing_offset += 1;

            let param_count = value >> 8;
            let cmd = value & 0x7F;

            // println!("gx regs: {} {cmd:x} {param_count}", unsafe { FUNC_NAME_LUT.get_unchecked(cmd) });

            match cmd {
                0x11 | 0x12 => self.mtx_push_pop_queue += 1,
                0x50 => self.flushed = true,
                _ => {}
            }

            self.processing_offset += param_count;

            self.cmd_fifo_len -= param_count as u16;
            self.cmd_fifo_len -= ((param_count as u32).wrapping_sub(1) >> 31) as u16;

            executed_cycles += 8;
            can_run_cmds = self.can_run_cmds();
            can_run_cmds && executed_cycles < cycle_diff && cmd != 0x50
        } {}

        if is_cmd_fifo_half_full && !self.is_cmd_fifo_half_full() {
            io_dma!(emu, ARM9).trigger_all(DmaTransferMode::GeometryCmdFifo, get_cm_mut!(emu));
        }

        let irq = u8::from(self.gx_stat.cmd_fifo_irq());
        if (irq == 1 && !self.is_cmd_fifo_half_full()) || (irq == 2 && self.is_cmd_fifo_empty()) {
            get_cpu_regs_mut!(emu, ARM9).send_interrupt(InterruptFlag::GeometryCmdFifo, emu);
        }

        if !self.is_cmd_fifo_full() {
            get_cpu_regs_mut!(emu, ARM9).unhalt(1);
        }

        if !can_run_cmds || self.flushed {
            self.queue_geometry(emu)
        }
    }

    fn queue_geometry(&mut self, emu: &mut Emu) {
        // println!("queue geometry");
        let gpu = &mut get_common_mut!(emu).gpu;
        {
            let geometry = gpu.get_3d_geometry_mut();
            if geometry.processing.load(Ordering::Acquire) {
                return;
            }

            // println!("queue geometry {} {}", self.processing_offset, self.cmd_fifo.len());

            mem::swap(&mut geometry.cmds, &mut self.cmd_fifo);
            self.cmd_fifo.clear();
            self.cmd_fifo.extend(&geometry.cmds[self.processing_offset..]);
            geometry.cmds_end = self.processing_offset;
            self.processing_offset = 0;

            self.geometry_sync(gpu.get_3d_geometry_mut());
            geometry.processing.store(true, Ordering::Release);
            // unsafe { vitasdk_sys::sceKernelSendSignal(geometry.thread_id) };
        }
    }

    fn geometry_sync(&mut self, geometry: &mut Gpu3DGeometry) {
        if self.acknowledge_error {
            geometry.gx_stat.set_proj_mtx_stack_lvl(u1::new(0));
            geometry.gx_stat.set_mtx_stack_overflow_underflow_err(false);
            self.acknowledge_error = false;
        }

        let mask = 0xC0000000;
        self.gx_stat = ((u32::from(self.gx_stat) & mask) | (u32::from(geometry.gx_stat) & !mask)).into();

        // println!("geometry sync mtx queue {} {}", self.mtx_push_pop_queue, geometry.executed_mtx_push_pop);
        self.mtx_push_pop_queue -= geometry.executed_mtx_push_pop;
        self.test_queue -= geometry.executed_tests;
        geometry.executed_mtx_push_pop = 0;
        geometry.executed_tests = 0;

        self.vertices_size = geometry.vertices_flushed_size;
        self.polygons_size = geometry.polygons_flushed_size;

        geometry.needs_sync = false;
    }

    fn post_queue_entry(&self, emu: &mut Emu) {
        if unlikely(self.is_cmd_fifo_full()) {
            get_mem_mut!(emu).breakout_imm = true;
            get_cpu_regs_mut!(emu, ARM9).halt(1);
        }
    }

    pub fn get_clip_mtx_result(&mut self, index: usize, emu: &mut Emu) -> u32 {
        // println!(
        //     "get clip mtx result {index} {:x} busy {}",
        //     get_common_mut!(emu).gpu.get_3d_geometry_mut().get_clip_mtx()[index] as u32,
        //     self.can_run_cmds()
        // );
        get_common_mut!(emu).gpu.get_3d_geometry_mut().get_clip_mtx()[index] as u32
    }

    pub fn get_vec_mtx_result(&self, index: usize, emu: &mut Emu) -> u32 {
        get_common!(emu).gpu.get_3d_geometry().matrices.dir[(index / 3) * 4 + index % 3] as u32
    }

    pub fn get_gx_stat(&self, emu: &mut Emu) -> u32 {
        let mut gx_stat = self.gx_stat;
        gx_stat.set_geometry_busy(
            self.can_run_cmds() || {
                let geometry = get_common!(emu).gpu.get_3d_geometry();
                geometry.processing.load(Ordering::Acquire) || geometry.needs_sync
            },
        );
        gx_stat.set_num_entries_cmd_fifo(u9::new(self.get_cmd_fifo_len() as u16));
        gx_stat.set_cmd_fifo_less_half_full(!self.is_cmd_fifo_half_full());
        gx_stat.set_cmd_fifo_empty(self.is_cmd_fifo_empty());
        gx_stat.set_box_pos_vec_test_busy(self.test_queue != 0);
        gx_stat.set_mtx_stack_busy(self.mtx_push_pop_queue != 0);
        // println!(
        //     "gx stat geometry busy {} mtx busy {} test busy {}",
        //     gx_stat.geometry_busy(),
        //     gx_stat.mtx_stack_busy(),
        //     gx_stat.box_pos_vec_test_busy()
        // );
        u32::from(gx_stat)
    }

    pub fn get_ram_count(&self) -> u32 {
        ((self.vertices_size as u32) << 16) | (self.polygons_size as u32)
    }

    pub fn get_pos_result(&self, index: usize, emu: &mut Emu) -> u32 {
        get_common!(emu).gpu.get_3d_geometry().pos_result[index] as u32
    }

    pub fn get_vec_result(&self, index: usize, emu: &mut Emu) -> u16 {
        get_common!(emu).gpu.get_3d_geometry().vec_result[index] as u16
    }

    fn queue_packed_value(&mut self, value: u32) {
        if self.last_cmd == 0 {
            if value != 0 {
                self.last_cmd = value;
                let cmd = value & 0x7F;
                self.cmd_remaining_params = unsafe { *FIFO_PARAM_COUNTS.get_unchecked(cmd as usize) };
                self.cmd_fifo.push(cmd | ((self.cmd_remaining_params as u32) << 8));
                self.test_queue += (cmd >= 0x70 && cmd <= 0x72) as u8;
                self.cmd_fifo_len += ((self.cmd_remaining_params as u32).wrapping_sub(1) >> 31) as u16;
            } else {
                return;
            }
        } else {
            self.cmd_remaining_params -= 1;
            self.cmd_fifo.push(value);
            self.cmd_fifo_len += 1;
        }

        while self.cmd_remaining_params == 0 {
            self.last_cmd >>= 8;
            if self.last_cmd != 0 {
                let cmd = self.last_cmd & 0x7F;
                self.cmd_remaining_params = unsafe { *FIFO_PARAM_COUNTS.get_unchecked(cmd as usize) };
                self.cmd_fifo.push(cmd | ((self.cmd_remaining_params as u32) << 8));
                self.test_queue += (cmd >= 0x70 && cmd <= 0x72) as u8;
                self.cmd_fifo_len += ((self.cmd_remaining_params as u32).wrapping_sub(1) >> 31) as u16;
            } else {
                break;
            }
        }
    }

    pub fn set_gx_fifo(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_packed_value(value & mask);
        self.post_queue_entry(emu);
    }

    pub fn set_gx_fifo_multiple(&mut self, values: &[u32], emu: &mut Emu) {
        for &value in values {
            self.queue_packed_value(value);
        }
        self.post_queue_entry(emu);
    }

    fn queue_unpacked_value<const CMD: u8>(&mut self, value: u32, emu: &mut Emu) {
        if self.cmd_remaining_params == 0 {
            self.cmd_remaining_params = FIFO_PARAM_COUNTS[CMD as usize];
            self.cmd_fifo.push(CMD as u32 | ((self.cmd_remaining_params as u32) << 8));
            if self.cmd_remaining_params > 0 {
                self.cmd_remaining_params -= 1;
                self.cmd_fifo.push(value);
            }

            match CMD {
                0x70 | 0x71 | 0x72 => self.test_queue += 1,
                _ => {}
            }
        } else {
            self.cmd_remaining_params -= 1;
            self.cmd_fifo.push(value);
        }
        self.cmd_fifo_len += 1;
        self.post_queue_entry(emu);
    }

    unpacked_cmd!(mtx_mode, 0x10);
    unpacked_cmd!(mtx_push, 0x11);
    unpacked_cmd!(mtx_pop, 0x12);
    unpacked_cmd!(mtx_store, 0x13);
    unpacked_cmd!(mtx_restore, 0x14);
    unpacked_cmd!(mtx_identity, 0x15);
    unpacked_cmd!(mtx_load44, 0x16);
    unpacked_cmd!(mtx_load43, 0x17);
    unpacked_cmd!(mtx_mult44, 0x18);
    unpacked_cmd!(mtx_mult43, 0x19);
    unpacked_cmd!(mtx_mult33, 0x1A);
    unpacked_cmd!(mtx_scale, 0x1B);
    unpacked_cmd!(mtx_trans, 0x1C);
    unpacked_cmd!(color, 0x20);
    unpacked_cmd!(normal, 0x21);
    unpacked_cmd!(tex_coord, 0x22);
    unpacked_cmd!(vtx16, 0x23);
    unpacked_cmd!(vtx10, 0x24);
    unpacked_cmd!(vtx_x_y, 0x25);
    unpacked_cmd!(vtx_x_z, 0x26);
    unpacked_cmd!(vtx_y_z, 0x27);
    unpacked_cmd!(vtx_diff, 0x28);
    unpacked_cmd!(polygon_attr, 0x29);
    unpacked_cmd!(tex_image_param, 0x2A);
    unpacked_cmd!(pltt_base, 0x2B);
    unpacked_cmd!(dif_amb, 0x30);
    unpacked_cmd!(spe_emi, 0x31);
    unpacked_cmd!(light_vector, 0x32);
    unpacked_cmd!(light_color, 0x33);
    unpacked_cmd!(shininess, 0x34);
    unpacked_cmd!(begin_vtxs, 0x40);
    unpacked_cmd!(end_vtxs, 0x41);
    unpacked_cmd!(swap_buffers, 0x50);
    unpacked_cmd!(viewport, 0x60);
    unpacked_cmd!(box_test, 0x70);
    unpacked_cmd!(pos_test, 0x71);
    unpacked_cmd!(vec_test, 0x72);

    pub fn set_gx_stat(&mut self, mut mask: u32, value: u32) {
        if value & (1 << 15) != 0 {
            self.acknowledge_error = true;
        }
        mask &= 0xC0000000;
        self.gx_stat = ((u32::from(self.gx_stat) & !mask) | (value & mask)).into();
    }
}

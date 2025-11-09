use crate::{core::graphics::gpu_3d::registers_3d::Gpu3DRegisters, fixed_fifo::FixedFifo, utils::HeapMemU32};
use dsvita_macros::gx_fifo_cmds;
use std::{cmp, hint::assert_unchecked, intrinsics::likely, mem};

const FIFO_SIZE: usize = 512;
const MAX_PARAM_COUNT: usize = 32;

pub struct GxCmdFifo {
    params: HeapMemU32<{ FIFO_SIZE * MAX_PARAM_COUNT }>,

    param_count: u8,
    param_remaining: u8,

    packed_cmd: u32,
    pub exe_fifo: FixedFifo<u8, { FIFO_SIZE as u16 }>,
    fifo_size: usize,
    pub test_queue: u8,
}

impl GxCmdFifo {
    pub fn new() -> Self {
        GxCmdFifo {
            params: HeapMemU32::new(),
            param_count: 0,
            param_remaining: 0,
            packed_cmd: 0,
            exe_fifo: FixedFifo::new(),
            fifo_size: 0,
            test_queue: 0,
        }
    }

    gx_fifo_cmds!(
        (0x10, 1, Gpu3DRegisters::exe_mtx_mode),
        (0x11, 0, Gpu3DRegisters::exe_mtx_push),
        (0x12, 1, Gpu3DRegisters::exe_mtx_pop),
        (0x13, 1, Gpu3DRegisters::exe_mtx_store),
        (0x14, 1, Gpu3DRegisters::exe_mtx_restore),
        (0x15, 0, Gpu3DRegisters::exe_mtx_identity),
        (0x16, 16, Gpu3DRegisters::exe_mtx_load44),
        (0x17, 12, Gpu3DRegisters::exe_mtx_load43),
        (0x18, 16, Gpu3DRegisters::exe_mtx_mult44),
        (0x19, 12, Gpu3DRegisters::exe_mtx_mult43),
        (0x1A, 9, Gpu3DRegisters::exe_mtx_mult33),
        (0x1B, 3, Gpu3DRegisters::exe_mtx_scale),
        (0x1C, 3, Gpu3DRegisters::exe_mtx_trans),
        (0x20, 1, Gpu3DRegisters::exe_color),
        (0x21, 1, Gpu3DRegisters::exe_normal) + frameskip,
        (0x22, 1, Gpu3DRegisters::exe_tex_coord) + frameskip,
        (0x23, 2, Gpu3DRegisters::exe_vtx16) + frameskip,
        (0x24, 1, Gpu3DRegisters::exe_vtx10) + frameskip,
        (0x25, 1, Gpu3DRegisters::exe_vtx_x_y) + frameskip,
        (0x26, 1, Gpu3DRegisters::exe_vtx_x_z) + frameskip,
        (0x27, 1, Gpu3DRegisters::exe_vtx_y_z) + frameskip,
        (0x28, 1, Gpu3DRegisters::exe_vtx_diff) + frameskip,
        (0x29, 1, Gpu3DRegisters::exe_polygon_attr),
        (0x2A, 1, Gpu3DRegisters::exe_tex_image_param),
        (0x2B, 1, Gpu3DRegisters::exe_pltt_base),
        (0x30, 1, Gpu3DRegisters::exe_dif_amb),
        (0x31, 1, Gpu3DRegisters::exe_spe_emi),
        (0x32, 1, Gpu3DRegisters::exe_light_vector),
        (0x33, 1, Gpu3DRegisters::exe_light_color),
        (0x34, 32, Gpu3DRegisters::exe_shininess),
        (0x40, 1, Gpu3DRegisters::exe_begin_vtxs) + frameskip,
        (0x41, 0, Gpu3DRegisters::exe_empty),
        (0x50, 1, Gpu3DRegisters::exe_swap_buffers),
        (0x60, 1, Gpu3DRegisters::exe_viewport),
        (0x70, 3, Gpu3DRegisters::exe_box_test) + test,
        (0x71, 2, Gpu3DRegisters::exe_pos_test) + test,
        (0x72, 1, Gpu3DRegisters::exe_vec_test) + test,
    );

    pub fn exe_queue(&mut self, cycles: u32, regs: &mut Gpu3DRegisters) {
        let pos = self.exe_fifo.pos_front() as usize;
        let cmd = *self.exe_fifo.front();
        self.exe_fifo.pop_front();
        let params = unsafe { mem::transmute(self.params.as_ptr().add(pos * MAX_PARAM_COUNT)) };
        let exe = unsafe { *Self::EXE_IN_QUEUE.get_unchecked(cmd as usize) };
        exe(self, params, cycles, regs);
    }

    fn push_cmd(&mut self, cmd: u8) {
        self.exe_fifo.push_back(cmd);
    }

    fn exe_cmd_in_queue_empty(&mut self, _: &[u32; MAX_PARAM_COUNT], cycles: u32, regs: &mut Gpu3DRegisters) {
        self.fifo_size -= 1;

        if cycles > 4 && !self.exe_fifo.is_empty() {
            self.exe_queue(cycles - 4, regs);
        }
    }

    fn exe_cmd_in_queue<const CMD: u8>(&mut self, params: &[u32; MAX_PARAM_COUNT], cycles: u32, regs: &mut Gpu3DRegisters) {
        let param_count = Self::PARAM_COUNT[CMD as usize];
        if param_count == 0 {
            self.fifo_size -= 1;
        } else {
            self.fifo_size -= param_count as usize;
        }
        if Self::IS_TEST[CMD as usize] {
            self.test_queue -= 1;
        }

        if !Self::CAN_FRAMESKIP[CMD as usize] || !regs.skip {
            let exe_func = Self::EXE[CMD as usize];
            exe_func(regs, params);
        }

        if CMD != 0x50 && cycles > 4 && !self.exe_fifo.is_empty() {
            self.exe_queue(cycles - 4, regs);
        }
    }

    #[inline(always)]
    fn single_next_cmd(&mut self) {
        self.packed_cmd >>= 8;
        if likely(self.packed_cmd != 0) {
            let init = Self::SINGLE_INIT[self.packed_cmd as usize & 0x7F];
            init(self);
        }
    }

    fn single_init<const CMD: u8>(&mut self) {
        if Self::IS_TEST[CMD as usize] {
            self.test_queue += 1;
        }

        let param_count = Self::PARAM_COUNT[CMD as usize];
        if param_count == 0 {
            self.fifo_size += 1;
            self.push_cmd((self.packed_cmd & 0x7F) as u8);
            self.single_next_cmd();
        } else {
            self.param_count = param_count;
            self.param_remaining = param_count;
        }
    }

    fn single(&mut self, value: u32) {
        let pos = self.exe_fifo.pos_end() as usize;
        let params_written = self.param_count - self.param_remaining;
        unsafe { *self.params.get_unchecked_mut(pos * MAX_PARAM_COUNT + params_written as usize) = value };
        self.param_remaining -= 1;
        self.fifo_size += 1;
        if self.param_remaining == 0 {
            self.push_cmd((self.packed_cmd & 0x7F) as u8);
            self.single_next_cmd();
        }
    }

    fn new_cmd_single(&mut self, value: u32) {
        self.packed_cmd = value;
        let init = Self::SINGLE_INIT[value as usize & 0x7F];
        init(self);
    }

    #[inline(always)]
    fn multiple_next_cmd(&mut self, values: &[u32]) {
        self.packed_cmd >>= 8;
        if likely(self.packed_cmd != 0) {
            let init = Self::MULTIPLE_INIT[self.packed_cmd as usize & 0x7F];
            init(self, values);
        } else if !values.is_empty() {
            self.new_cmd_multiple(values);
        }
    }

    fn multiple_init<const CMD: u8>(&mut self, values: &[u32]) {
        if Self::IS_TEST[CMD as usize] {
            self.test_queue += 1;
        }

        let param_count = Self::PARAM_COUNT[CMD as usize];
        if param_count == 0 {
            self.fifo_size += 1;
            self.push_cmd((self.packed_cmd & 0x7F) as u8);
            self.multiple_next_cmd(values);
        } else if param_count as usize <= values.len() {
            let pos_end = self.exe_fifo.pos_end() as usize;
            for i in 0..param_count as usize {
                unsafe { *self.params.get_unchecked_mut(pos_end * MAX_PARAM_COUNT + i) = values[i] };
            }
            self.fifo_size += param_count as usize;
            self.push_cmd((self.packed_cmd & 0x7F) as u8);
            self.multiple_next_cmd(&values[param_count as usize..]);
        } else {
            self.param_count = param_count;
            self.param_remaining = param_count - values.len() as u8;
            let pos_end = self.exe_fifo.pos_end() as usize;
            for i in 0..values.len() {
                unsafe { *self.params.get_unchecked_mut(pos_end * MAX_PARAM_COUNT + i) = values[i] };
            }
            self.fifo_size += values.len() as usize;
        }
    }

    fn multiple(&mut self, values: &[u32]) {
        unsafe { assert_unchecked(!values.is_empty()) };

        let pos_end = self.exe_fifo.pos_end() as usize;
        let params_to_write = cmp::min(self.param_remaining as usize, values.len());
        let params_written = self.param_count - self.param_remaining;
        let params_offset = pos_end * MAX_PARAM_COUNT + params_written as usize;

        for i in 0..params_to_write {
            unsafe { *self.params.get_unchecked_mut(params_offset + i) = values[i] };
        }

        self.param_remaining -= params_to_write as u8;
        self.fifo_size += params_to_write;
        if self.param_remaining == 0 {
            self.push_cmd((self.packed_cmd & 0x7F) as u8);
            self.multiple_next_cmd(&values[params_to_write..]);
        }
    }

    fn new_cmd_multiple(&mut self, values: &[u32]) {
        unsafe { assert_unchecked(!values.is_empty()) };
        self.packed_cmd = values[0];
        let init = Self::MULTIPLE_INIT[values[0] as usize & 0x7F];
        init(self, &values[1..]);
    }

    pub(super) fn write(&mut self, value: u32) {
        if self.param_remaining == 0 {
            self.new_cmd_single(value);
        } else {
            self.single(value);
        }
    }

    pub(super) fn write_multiple(&mut self, values: &[u32]) {
        if self.param_remaining == 0 {
            self.new_cmd_multiple(values);
        } else {
            self.multiple(values);
        }
    }

    pub fn unpacked_cmd<const CMD: u8>(&mut self, value: u32) {
        let param_count = Self::PARAM_COUNT[CMD as usize];
        if self.param_remaining == 0 {
            self.fifo_size += 1;
            if param_count > 0 {
                let pos = self.exe_fifo.pos_end() as usize;
                unsafe { *self.params.get_unchecked_mut(pos * MAX_PARAM_COUNT) = value };
            }

            if param_count <= 1 {
                self.push_cmd(CMD);
            } else {
                self.param_remaining = param_count - 1;
            }
        } else {
            let pos = self.exe_fifo.pos_end() as usize;
            let params_written = param_count - self.param_remaining;
            unsafe { *self.params.get_unchecked_mut(pos * MAX_PARAM_COUNT + params_written as usize) = value };
            self.param_remaining -= 1;
            self.fifo_size += 1;
            if self.param_remaining == 0 {
                self.push_cmd(CMD);
            }
        }
    }

    pub fn is_fifo_full(&self) -> bool {
        self.fifo_size >= 260
    }

    pub fn is_fifo_half_full(&self) -> bool {
        self.fifo_size >= 132
    }

    pub fn is_fifo_empty(&self) -> bool {
        self.fifo_size <= 4
    }

    pub fn get_fifo_len(&self) -> usize {
        cmp::max(self.fifo_size as isize - 4, 0) as usize
    }
}

use crate::core::emu::NitroSdkVersion;
use crate::core::memory::regions::{self, OAM_OFFSET};
use crate::core::CpuType::ARM9;
use crate::core::{div_sqrt, CpuType};
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::inst_branch_handler::check_scheduler;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm::JitAsm;
use crate::jit::jit_asm_common_funs::exit_guest_context;
use crate::jit::jit_memory::JitEntry;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::logging::{debug_println, info_println};
use crate::settings::Arm7Emu;
use crate::{cartridge_io, get_jit_asm_ptr, IS_DEBUG};
use std::cmp::min;
use std::intrinsics::{likely, unlikely};
use std::mem::MaybeUninit;
use std::{mem, slice};
use CpuType::ARM7;

const GX_FIFO_NOP_CLEAR128: [u32; 37] = [
    0xe3a01000, 0xe3a02000, 0xe3a03000, 0xe3a0c000, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e,
    0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e,
    0xe880100e, 0xe880100e, 0xe880100e, 0xe880100e, 0xe12fff1e,
];

const GX_FIFO_SEND48B: [u32; 9] = [0xe8b0100c, 0xe881100c, 0xe8b0100c, 0xe881100c, 0xe8b0100c, 0xe881100c, 0xe8b0100c, 0xe881100c, 0xe12fff1e];
const GX_FIFO_SEND64B: [u32; 7] = [0xe92d01f0, 0xe8b011fc, 0xe88111fc, 0xe8b011fc, 0xe88111fc, 0xe8bd01f0, 0xe12fff1e];
const GX_FIFO_SEND128B: [u32; 11] = [
    0xe92d01f0, 0xe8b011fc, 0xe88111fc, 0xe8b011fc, 0xe88111fc, 0xe8b011fc, 0xe88111fc, 0xe8b011fc, 0xe88111fc, 0xe8bd01f0, 0xe12fff1e,
];

const MI_CPU_CLEAR16: [u32; 6] = [0xe3a03000, 0xe1530002, 0xb18100b3, 0xb2833002, 0xbafffffb, 0xe12fff1e];
const MI_CPU_CLEAR32: [u32; 5] = [0xe081c002, 0xe151000c, 0xb8a10001, 0xbafffffc, 0xe12fff1e];
const MI_CPU_CLEARFAST: [u32; 19] = [
    0xe92d03f0, 0xe0819002, 0xe1a0c2a2, 0xe081c28c, 0xe1a02000, 0xe1a03002, 0xe1a04002, 0xe1a05002, 0xe1a06002, 0xe1a07002, 0xe1a08002, 0xe151000c, 0xb8a101fd, 0xbafffffc, 0xe1510009, 0xb8a10001,
    0xbafffffc, 0xe8bd03f0, 0xe12fff1e,
];

const MI_CPU_COPY16: [u32; 7] = [0xe3a0c000, 0xe15c0002, 0xb19030bc, 0xb18130bc, 0xb28cc002, 0xbafffffa, 0xe12fff1e];
const MI_CPU_COPY32: [u32; 6] = [0xe081c002, 0xe151000c, 0xb8b00004, 0xb8a10004, 0xbafffffb, 0xe12fff1e];

const MI_CPU_SEND32: [u32; 6] = [0xe080c002, 0xe150000c, 0xb8b00004, 0xb5812000, 0xbafffffb, 0xe12fff1e];

const MI_CPU_FILL8: [u32; 37] = [
    0xe3520000, 0x12fff1e, 0xe3100001, 0xa000006, 0xe150c0b1, 0xe20cc0ff, 0xe18c3401, 0xe14030b1, 0xe2800001, 0xe2522001, 0x12fff1e, 0xe3520002, 0x3a00000f, 0xe1811401, 0xe3100002, 0xa000002,
    0xe0c010b2, 0xe2522002, 0x12fff1e, 0xe1811801, 0xe3d23003, 0xa000004, 0xe0422003, 0xe083c000, 0xe4801004, 0xe150000c, 0x3afffffc, 0xe3120002, 0x10c010b2, 0xe3120001, 0x12fff1e, 0xe1d030b0,
    0xe2033cff, 0xe20110ff, 0xe1811003, 0xe1c010b0, 0xe12fff1e,
];

const MI_COPY64B: [u32; 11] = [
    0xe8b0100c, 0xe8a1100c, 0xe8b0100c, 0xe8a1100c, 0xe8b0100c, 0xe8a1100c, 0xe8b0100c, 0xe8a1100c, 0xe890100d, 0xe8a1100d, 0xe12fff1e,
];

const CP_SAVE_CONTEXT: [u32; 15] = [
    0xe59f1034, 0xe92d0010, 0xe891101c, 0xe8a0101c, 0xe151c1b0, 0xe2811028, 0xe891000c, 0xe8a0000c, 0xe20cc003, 0xe15120b8, 0xe1c0c0b0, 0xe2022001, 0xe1c020b2, 0xe8bd0010, 0xe12fff1e,
];
const CP_RESTORE_CONTEXT: [u32; 14] = [
    0xe92d0010, 0xe59f102c, 0xe890101c, 0xe881101c, 0xe1d021b8, 0xe1d031ba, 0xe14121b0, 0xe1c132b0, 0xe2800010, 0xe2811028, 0xe890000c, 0xe881000c, 0xe8bd0010, 0xe12fff1e,
];

const MICROCODE_SHAKEHAND: [u32; 10] = [0xe1d120b0, 0xe1d030b0, 0xe2833001, 0xe1c030b0, 0xe1d1c0b0, 0xe152000c, 0x0afffffa, 0xe2833001, 0xe1c030b0, 0xe12fff1e];
const MICROCODE_WAIT_AGREEMENT: [u32; 7] = [0xe1d020b0, 0xe1510002, 0x012fff1e, 0xe3a03010, 0xe2533001, 0x1afffffd, 0xeafffff8];

struct Function {
    opcodes: &'static [u32],
    name: &'static str,
    hle_function: unsafe extern "C" fn(u32),
}

impl Function {
    const fn new(opcodes: &'static [u32], name: &'static str, hle_function: unsafe extern "C" fn(u32)) -> Self {
        Function { opcodes, name, hle_function }
    }
}

impl PartialEq<[InstInfo]> for Function {
    fn eq(&self, other: &[InstInfo]) -> bool {
        if self.opcodes.len() != other.len() {
            return false;
        }

        for i in 0..self.opcodes.len() {
            if self.opcodes[i] != other[i].opcode {
                return false;
            }
        }
        true
    }
}

unsafe fn get_buf<T>(len: usize) -> &'static mut [T] {
    static mut BUF: Vec<u8> = Vec::new();
    let byte_len = len * size_of::<T>();
    if unlikely(BUF.len() < byte_len) {
        BUF.reserve(byte_len - BUF.len());
        BUF.set_len(byte_len);
    }
    slice::from_raw_parts_mut(BUF.as_mut_ptr() as *mut T, len)
}

unsafe fn hle_post_function<const CPU: CpuType>(asm: &mut JitAsm, cycles: u32, guest_pc: u32) {
    asm.runtime_data.accumulated_cycles += min(cycles, (u16::MAX - asm.runtime_data.accumulated_cycles - 1) as u32) as u16;
    asm.emu.breakout_imm = false;

    let regs = CPU.thread_regs();
    let lr = regs.lr;
    let desired_lr = asm.runtime_data.pop_return_stack();
    regs.pc = lr;
    asm.emu.thread_set_thumb(CPU, lr & 1 == 1);
    if asm.emu.settings.arm7_emu() == Arm7Emu::Hle {
        check_scheduler::<CPU, true>(asm, guest_pc);
    } else {
        check_scheduler::<CPU, false>(asm, guest_pc);
    }

    if unlikely(lr != desired_lr) {
        if IS_DEBUG {
            asm.runtime_data.set_branch_out_pc(guest_pc);
        }
        exit_guest_context!(asm);
    }
}

unsafe extern "C" fn hle_mi_cpu_clear32<const CPU: CpuType>(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    let regs = CPU.thread_regs();
    let value = regs.gp_regs[0];
    let dst = regs.gp_regs[1];
    let len = regs.gp_regs[2] >> 2;

    if likely(len > 0) {
        asm.emu.mem_write_multiple_memset::<CPU, true, u32>(dst, value, len as usize);
    }

    hle_post_function::<CPU>(asm, 1 + len * 6 + 3, guest_pc);
}

unsafe extern "C" fn hle_mi_cpu_clear16<const CPU: CpuType>(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    let regs = CPU.thread_regs();
    let value = regs.gp_regs[0] as u16;
    let dst = regs.gp_regs[1];
    let len = regs.gp_regs[2] >> 1;

    if likely(len > 0) {
        asm.emu.mem_write_multiple_memset::<CPU, true, u16>(dst, value, len as usize);
    }

    hle_post_function::<CPU>(asm, 1 + len * 7 + 3, guest_pc);
}

unsafe extern "C" fn hle_mi_cpu_copy32<const CPU: CpuType>(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    let regs = CPU.thread_regs();
    let src = regs.gp_regs[0];
    let dst = regs.gp_regs[1];
    let len = regs.gp_regs[2] as usize >> 2;

    if likely(len > 0) {
        let aligned_addr = src & !0x3;
        let aligned_addr = aligned_addr & 0x0FFFFFFF;
        let shm_offset = asm.emu.get_shm_offset::<CPU, true, false>(aligned_addr);
        let values = if likely(shm_offset != 0) {
            slice::from_raw_parts(asm.emu.mem.shm.as_ptr().add(shm_offset) as *const u32, len)
        } else {
            let buf = get_buf(len);
            asm.emu.mem_read_multiple_slice::<CPU, true, false, u32>(aligned_addr, buf);
            buf
        };

        asm.emu.mem_write_multiple_slice::<CPU, true, u32>(dst, values);
    }

    hle_post_function::<CPU>(asm, 1 + len as u32 * 9 + 3, guest_pc);
}

unsafe extern "C" fn hle_mi_cpu_send32<const CPU: CpuType>(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    let regs = CPU.thread_regs();
    let src = regs.gp_regs[0];
    let dst = regs.gp_regs[1];
    let len = regs.gp_regs[2] as usize >> 2;

    if likely(len > 0) {
        let aligned_addr = src & !0x3;
        let aligned_addr = aligned_addr & 0x0FFFFFFF;
        let shm_offset = asm.emu.get_shm_offset::<CPU, true, false>(aligned_addr);
        let values = if likely(shm_offset != 0) {
            slice::from_raw_parts(asm.emu.mem.shm.as_ptr().add(shm_offset) as *const u32, len)
        } else {
            let buf = get_buf(len);
            asm.emu.mem_read_multiple_slice::<CPU, true, false, u32>(aligned_addr, buf);
            buf
        };

        if CPU == ARM9 && likely(dst >= 0x4000400 && dst < 0x4000440) {
            asm.emu.regs_3d_set_gx_fifo_multiple(values);
        } else {
            asm.emu.mem_write_multiple_slice::<CPU, true, u32>(dst, values);
        }
    }

    hle_post_function::<CPU>(asm, 1 + len as u32 * 9 + 3, guest_pc);
}

unsafe extern "C" fn hle_mi_cpu_copy16<const CPU: CpuType>(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    let regs = CPU.thread_regs();
    let src = regs.gp_regs[0];
    let dst = regs.gp_regs[1];
    let len = regs.gp_regs[2] as usize >> 1;

    if likely(len > 0) {
        let aligned_addr = src & !0x1;
        let aligned_addr = aligned_addr & 0x0FFFFFFF;
        let shm_offset = asm.emu.get_shm_offset::<CPU, true, false>(aligned_addr);
        let values = if likely(shm_offset != 0) {
            slice::from_raw_parts(asm.emu.mem.shm.as_ptr().add(shm_offset) as *const u16, len)
        } else {
            let buf = get_buf(len);
            asm.emu.mem_read_multiple_slice::<CPU, true, false, u16>(aligned_addr, buf);
            buf
        };

        asm.emu.mem_write_multiple_slice::<CPU, true, u16>(dst, values);
    }

    hle_post_function::<CPU>(asm, 1 + len as u32 * 10 + 3, guest_pc);
}

unsafe extern "C" fn hle_mi_copy64b<const CPU: CpuType>(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    let regs = CPU.thread_regs();
    let src = regs.gp_regs[0];
    let dst = regs.gp_regs[1];

    if CPU == ARM9 && likely(src == 0x4000640) {
        let clip_matrix = asm.emu.gpu.gpu_3d_regs.get_clip_matrix();
        let clip_matrix = slice::from_raw_parts(clip_matrix.0.as_ptr() as *const u32, 16);
        asm.emu.mem_write_multiple_slice::<CPU, true, u32>(dst, clip_matrix);
    } else {
        let mut buf: [u32; 16] = MaybeUninit::uninit().assume_init();
        let mut buf = &mut buf;

        let aligned_addr = src & !0x3;
        let aligned_addr = aligned_addr & 0x0FFFFFFF;
        let shm_offset = asm.emu.get_shm_offset::<CPU, true, false>(aligned_addr);
        if shm_offset != 0 {
            buf = mem::transmute(asm.emu.mem.shm.as_ptr().add(shm_offset) as *const u32);
        } else {
            asm.emu.mem_read_multiple_slice::<CPU, true, false, u32>(aligned_addr, buf);
        }

        asm.emu.mem_write_multiple_slice::<CPU, true, u32>(dst, buf);
    }

    hle_post_function::<CPU>(asm, 50, guest_pc);
}

unsafe extern "C" fn hle_mi_cpu_clearfast<const CPU: CpuType>(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    let regs = CPU.thread_regs();
    let value = regs.gp_regs[0];
    let dst = regs.gp_regs[1];
    let len = regs.gp_regs[2] >> 2;

    if likely(len > 0) {
        asm.emu.mem_write_multiple_memset::<CPU, true, u32>(dst, value, len as usize);
    }

    hle_post_function::<CPU>(asm, 17 + len * 6 + 8, guest_pc);
}

unsafe extern "C" fn hle_mi_cpu_fill8<const CPU: CpuType>(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    let regs = CPU.thread_regs();
    let dst = regs.gp_regs[0];
    let value = regs.gp_regs[1] as u8;
    let len = regs.gp_regs[2];

    if likely(len > 0) {
        asm.emu.mem_write_multiple_memset::<CPU, true, u8>(dst, value, len as usize);
    }

    hle_post_function::<CPU>(asm, len, guest_pc);
}

unsafe extern "C" fn hle_gx_fifo_nop_clear128(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    let regs = ARM9.thread_regs();
    let dst = regs.gp_regs[0];

    debug_assert_eq!(dst, 0x4000400);

    let nops = [0; 128];
    asm.emu.regs_3d_set_gx_fifo_multiple(&nops);

    hle_post_function::<{ ARM9 }>(asm, 167, guest_pc);
}

unsafe extern "C" fn hle_gx_fifo_send64b(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    let regs = ARM9.thread_regs();
    let src = regs.gp_regs[0];
    let dst = regs.gp_regs[1];

    debug_assert_eq!(dst, 0x4000400);

    let mut buf: [u32; 16] = MaybeUninit::uninit().assume_init();
    let mut buf = &mut buf;

    let aligned_addr = src & !0x3;
    let aligned_addr = aligned_addr & 0x0FFFFFFF;
    let shm_offset = asm.emu.get_shm_offset::<{ ARM9 }, true, false>(aligned_addr);
    if shm_offset != 0 {
        buf = mem::transmute(asm.emu.mem.shm.as_ptr().add(shm_offset) as *const u32);
    } else {
        asm.emu.mem_read_multiple_slice::<{ ARM9 }, true, false, u32>(aligned_addr, buf);
    }

    asm.emu.regs_3d_set_gx_fifo_multiple(buf);

    hle_post_function::<{ ARM9 }>(asm, 44, guest_pc);
}

unsafe extern "C" fn hle_gx_fifo_send48b(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    let regs = ARM9.thread_regs();
    let src = regs.gp_regs[0];
    let dst = regs.gp_regs[1];

    debug_assert_eq!(dst, 0x4000400);

    let mut buf: [u32; 12] = MaybeUninit::uninit().assume_init();
    let mut buf = &mut buf;

    let aligned_addr = src & !0x3;
    let aligned_addr = aligned_addr & 0x0FFFFFFF;
    let shm_offset = asm.emu.get_shm_offset::<{ ARM9 }, true, false>(aligned_addr);
    if shm_offset != 0 {
        buf = mem::transmute(asm.emu.mem.shm.as_ptr().add(shm_offset) as *const u32);
    } else {
        asm.emu.mem_read_multiple_slice::<{ ARM9 }, true, false, u32>(aligned_addr, buf);
    }

    asm.emu.regs_3d_set_gx_fifo_multiple(buf);

    hle_post_function::<{ ARM9 }>(asm, 39, guest_pc);
}

unsafe extern "C" fn hle_gx_fifo_send128b(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    let regs = ARM9.thread_regs();
    let src = regs.gp_regs[0];
    let dst = regs.gp_regs[1];

    debug_assert_eq!(dst, 0x4000400);

    let mut buf: [u32; 32] = MaybeUninit::uninit().assume_init();
    let mut buf = &mut buf;

    let aligned_addr = src & !0x3;
    let aligned_addr = aligned_addr & 0x0FFFFFFF;
    let shm_offset = asm.emu.get_shm_offset::<{ ARM9 }, true, false>(aligned_addr);
    if shm_offset != 0 {
        buf = mem::transmute(asm.emu.mem.shm.as_ptr().add(shm_offset) as *const u32);
    } else {
        asm.emu.mem_read_multiple_slice::<{ ARM9 }, true, false, u32>(aligned_addr, buf);
    }

    asm.emu.regs_3d_set_gx_fifo_multiple(buf);

    hle_post_function::<{ ARM9 }>(asm, 82, guest_pc);
}

unsafe extern "C" fn hle_cp_save_context(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    let regs = ARM9.thread_regs();
    let cp_context_addr = regs.gp_regs[0];

    let aligned_addr = cp_context_addr & !0x3;
    let aligned_addr = aligned_addr & 0x0FFFFFFF;
    // Don't query write offset here, we don't want to invalidate the jit block
    let shm_offset = asm.emu.get_shm_offset::<{ ARM9 }, true, false>(aligned_addr);
    debug_assert_ne!(shm_offset, 0);

    let cp_context: &mut div_sqrt::CpContext = mem::transmute(asm.emu.mem.shm.as_mut_ptr().add(shm_offset));
    asm.emu.div_sqrt.get_context(cp_context);

    hle_post_function::<{ ARM9 }>(asm, 42, guest_pc);
}

unsafe extern "C" fn hle_cp_restore_context(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    let regs = ARM9.thread_regs();
    let cp_context_addr = regs.gp_regs[0];

    let aligned_addr = cp_context_addr & !0x3;
    let aligned_addr = aligned_addr & 0x0FFFFFFF;
    let shm_offset = asm.emu.get_shm_offset::<{ ARM9 }, true, false>(aligned_addr);
    debug_assert_ne!(shm_offset, 0);

    let cp_context: &div_sqrt::CpContext = mem::transmute(asm.emu.mem.shm.as_ptr().add(shm_offset));
    asm.emu.div_sqrt.set_context(cp_context);

    hle_post_function::<{ ARM9 }>(asm, 41, guest_pc);
}

unsafe extern "C" fn hle_os_irqhandler(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    let regs = ARM9.thread_regs();

    let irqs = regs.ie & regs.irf;
    if regs.ime == 0 || irqs == 0 {
        hle_post_function::<{ ARM9 }>(asm, 18, guest_pc);
    }

    let irq_to_handle = irqs.trailing_zeros();
    regs.irf &= !(1 << irq_to_handle);

    regs.sp -= 4;
    asm.emu.mem_write::<{ ARM9 }, u32>(regs.sp, regs.lr);

    regs.lr = asm.emu.os_irq_handler_thread_switch_addr;

    let irq_table_addr = asm.emu.os_irq_table_addr;
    let irq_func = asm.emu.mem_read::<{ ARM9 }, u32>(irq_table_addr + (irq_to_handle << 2));

    regs.gp_regs[0] = irq_func;
    regs.gp_regs[1] = irq_table_addr;
    regs.gp_regs[2] = 1 << irq_to_handle;
    regs.gp_regs[3] = 0x80000000;
    regs.gp_regs[12] = 0x4000210;

    let jit_entry: extern "C" fn(u32) = mem::transmute(asm.emu.jit.get_jit_start_addr(irq_func));
    jit_entry(irq_func);
}

unsafe extern "C" fn fs_clear_overlay_image_hook() {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    let regs = ARM9.thread_regs();
    let overlay_info_header_addr = regs.gp_regs[0];

    let aligned_addr = overlay_info_header_addr & !0x3;
    let aligned_addr = aligned_addr & 0x0FFFFFFF;
    let shm_offset = asm.emu.get_shm_offset::<{ ARM9 }, true, false>(aligned_addr);
    debug_assert_ne!(shm_offset, 0);

    let overlay_info_header: &cartridge_io::FsOverlayInfoHeader = mem::transmute(asm.emu.mem.shm.as_ptr().add(shm_offset));
    asm.emu.jit.invalidate_blocks(overlay_info_header.ram_address, overlay_info_header.total_size() as usize);
}

unsafe extern "C" fn hle_microcode_shakehand(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    hle_post_function::<{ ARM9 }>(asm, 20, guest_pc);
}

unsafe extern "C" fn hle_microcode_wait_agreement(guest_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();
    hle_post_function::<{ ARM9 }>(asm, 7, guest_pc);
}

const FUNCTIONS_ARM9: &[Function] = &[
    Function::new(&MI_CPU_CLEAR32, "MI_CPU_CLEAR32", hle_mi_cpu_clear32::<{ ARM9 }>),
    Function::new(&MI_CPU_CLEAR16, "MI_CPU_CLEAR16", hle_mi_cpu_clear16::<{ ARM9 }>),
    // Function::new(&MI_CPU_COPY32, "MI_CPU_COPY32", hle_mi_cpu_copy32::<{ ARM9 }>),
    Function::new(&MI_CPU_SEND32, "MI_CPU_SEND32", hle_mi_cpu_send32::<{ ARM9 }>),
    Function::new(&MI_CPU_COPY16, "MI_CPU_COPY16", hle_mi_cpu_copy16::<{ ARM9 }>),
    Function::new(&GX_FIFO_SEND64B, "GX_FIFO_SEND64B", hle_gx_fifo_send64b),
    Function::new(&GX_FIFO_SEND48B, "GX_FIFO_SEND48B", hle_gx_fifo_send48b),
    Function::new(&GX_FIFO_SEND128B, "GX_FIFO_SEND128B", hle_gx_fifo_send128b),
    Function::new(&MI_COPY64B, "MI_COPY64B", hle_mi_copy64b::<{ ARM9 }>),
    Function::new(&CP_RESTORE_CONTEXT, "CP_RESTORE_CONTEXT", hle_cp_restore_context),
    Function::new(&CP_SAVE_CONTEXT, "CP_SAVE_CONTEXT", hle_cp_save_context),
    // Function::new(&MI_CPU_CLEARFAST, "MI_CPU_CLEARFAST", hle_mi_cpu_clearfast::<{ ARM9 }>),
    Function::new(&MI_CPU_FILL8, "MI_CPU_FILL8", hle_mi_cpu_fill8::<{ ARM9 }>),
    Function::new(&GX_FIFO_NOP_CLEAR128, "GX_FIFO_NOP_CLEAR128", hle_gx_fifo_nop_clear128),
];

const FUNCTIONS_ARM7: &[Function] = &[
    Function::new(&MI_CPU_CLEAR32, "MI_CPU_CLEAR32", hle_mi_cpu_clear32::<{ ARM7 }>),
    Function::new(&MI_CPU_CLEAR16, "MI_CPU_CLEAR16", hle_mi_cpu_clear16::<{ ARM7 }>),
    // Function::new(&MI_CPU_COPY32, "MI_CPU_COPY32", hle_mi_cpu_copy32::<{ ARM7 }>),
    Function::new(&MI_CPU_SEND32, "MI_CPU_SEND32", hle_mi_cpu_send32::<{ ARM7 }>),
    Function::new(&MI_CPU_COPY16, "MI_CPU_COPY16", hle_mi_cpu_copy16::<{ ARM7 }>),
    Function::new(&MI_COPY64B, "MI_COPY64B", hle_mi_copy64b::<{ ARM7 }>),
    // Function::new(&MI_CPU_CLEARFAST, "MI_CPU_CLEARFAST", hle_mi_cpu_clearfast::<{ ARM7 }>),
    Function::new(&MI_CPU_FILL8, "MI_CPU_FILL8", hle_mi_cpu_fill8::<{ ARM7 }>),
];

impl JitAsm<'_> {
    pub fn parse_nitrosdk_entry(&mut self) {
        let pc = self.cpu.thread_regs().pc;
        let mut insts = Vec::new();
        let mut cycle_counts = Vec::new();
        Self::fill_jit_insts_buf(self.cpu, &mut insts, &mut cycle_counts, self.emu, pc, false, true);
        if insts.len() < 3 {
            return;
        }

        let inst = &insts[0];
        match inst.op {
            Op::Mov => {
                if inst.operands()[0].as_reg_no_shift() != Some(Reg::R12) || inst.operands()[1].as_imm() != Some(0x4000000) {
                    return;
                }
            }
            _ => return,
        }

        let inst = &insts[1];
        match inst.op {
            Op::Str(transfer) => {
                if !transfer.pre()
                    || transfer.write_back()
                    || !transfer.add()
                    || transfer.size() != 2
                    || inst.operands()[0].as_reg_no_shift() != Some(Reg::R12)
                    || inst.operands()[1].as_reg_no_shift() != Some(Reg::R12)
                    || inst.operands()[2].as_imm() != Some(0x208)
                {
                    return;
                }
            }
            _ => return,
        }

        let mut start_module_params_addr = 0;
        let mut oam_imm_load_found = false;
        let mut start_module_params_ldr_index = 0;
        for (i, inst) in insts.iter().enumerate() {
            let current_pc = pc + ((i as u32) << 2);
            match (inst.op, inst.imm_transfer_addr(current_pc)) {
                (Op::Ldr(transfer), Some(imm_addr)) if transfer.size() == 2 => {
                    let imm_value = self.emu.mem_read::<{ ARM9 }, u32>(imm_addr);
                    if oam_imm_load_found {
                        start_module_params_addr = imm_value;
                        start_module_params_ldr_index = i;
                        break;
                    } else if imm_value == OAM_OFFSET {
                        oam_imm_load_found = true;
                    }
                }
                _ => {}
            }
        }

        if start_module_params_addr == 0 {
            return;
        }

        let mut buf = [0; 3];
        self.emu.mem_read_multiple_slice::<{ ARM9 }, true, true, u32>(start_module_params_addr + 24, &mut buf);
        let sdk_version_info = buf[0];
        let sdk_nitro_code_be = buf[1];
        let sdk_nitro_code_le = buf[2];

        if sdk_nitro_code_le != u32::from_be(sdk_nitro_code_be) {
            return;
        }
        self.emu.nitro_sdk_version = NitroSdkVersion::from(sdk_version_info);
        info_println!("Found Nitro SDK version {:?}", self.emu.nitro_sdk_version);

        const ADD_IMM_VALUES: [u32; 2] = [0x3fc0, 0x3c];
        let mut add_match_count = 0;
        for i in (start_module_params_ldr_index + 1)..insts.len() {
            let inst = &insts[i];

            match inst.op {
                Op::Ldr(transfer) if transfer.size() == 2 => {
                    let current_pc = pc + ((i as u32) << 2);
                    if let Some(imm_addr) = inst.imm_transfer_addr(current_pc) {
                        if add_match_count == ADD_IMM_VALUES.len() {
                            let imm_value = self.emu.mem_read::<{ ARM9 }, u32>(imm_addr);
                            if imm_value < regions::MAIN_OFFSET {
                                self.os_irq_handler_addr = imm_value;
                                break;
                            }
                        }
                    }
                }
                Op::Add if add_match_count < ADD_IMM_VALUES.len() && inst.operands()[2].as_imm().is_some() => {
                    if ADD_IMM_VALUES[add_match_count] == inst.operands()[2].as_imm().unwrap() {
                        add_match_count += 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn is_invalidate_range(&mut self, guest_pc: u32, cp15_reg: u32) -> bool {
        let mut insts = Vec::new();
        let mut cycle_counts = Vec::new();
        Self::fill_jit_insts_buf(self.cpu, &mut insts, &mut cycle_counts, self.emu, guest_pc, false, true);

        let mut has_invalidate = false;
        let mut mcr_count = 0;
        for inst in insts {
            if matches!(inst.op, Op::Mcr) {
                let cn = (inst.opcode >> 16) & 0xF;
                let cm = inst.opcode & 0xF;
                let cp = (inst.opcode >> 5) & 0x7;
                let reg = (cn << 16) | (cm << 8) | cp;
                if reg == cp15_reg {
                    has_invalidate = true;
                }
                mcr_count += 1;
            }
        }
        has_invalidate && mcr_count == 1
    }

    fn is_addr_fs_clear_overlay_image(&mut self, guest_pc: u32, thumb: bool) -> bool {
        let mut insts = Vec::new();
        let mut cycle_counts = Vec::new();
        Self::fill_jit_insts_buf(self.cpu, &mut insts, &mut cycle_counts, self.emu, guest_pc, thumb, true);

        let pc_shift = if thumb { 1 } else { 2 };
        let mut has_ic_invalidate_range = false;
        let mut has_dc_invalidate_range = false;
        let mut bl_count = 0;
        for (i, inst) in insts.iter().enumerate() {
            if matches!(inst.op, Op::Bl | Op::BlxOffT) {
                bl_count += 1;

                let pc = guest_pc + ((i as u32) << pc_shift);
                let target_pc = if thumb {
                    if i == 0 || insts[i - 1].op != Op::BlSetupT {
                        continue;
                    }
                    let relative_pc = insts[i - 1].operands()[0].as_imm().unwrap() as i32 + 2;
                    let target_pc = (pc as i32 + relative_pc) as u32;
                    (target_pc + inst.operands()[0].as_imm().unwrap()) & !3
                } else {
                    let relative_pc = inst.operands()[0].as_imm().unwrap() as i32 + 8;
                    (pc as i32 + relative_pc) as u32
                };

                if !has_ic_invalidate_range {
                    if self.is_invalidate_range(target_pc, 0x070501) {
                        has_ic_invalidate_range = true;
                    }
                } else if !has_dc_invalidate_range {
                    if self.is_invalidate_range(target_pc, 0x070601) {
                        has_dc_invalidate_range = true;
                    }
                }
            }
        }
        has_dc_invalidate_range && bl_count == 3
    }

    pub fn emit_fs_clear_overlay_image_hook(&mut self, guest_pc: u32, thumb: bool, block_asm: &mut BlockAsm) {
        if self.cpu == ARM7 || !self.emu.nitro_sdk_version.is_valid() {
            return;
        }

        if self.emu.fs_clear_overlay_image_addr == 0 {
            let regs = ARM9.thread_regs();
            let ptr = regs.gp_regs[0];
            let shm_offset = self.emu.get_shm_offset::<{ ARM9 }, true, false>(ptr & 0x0FFFFFFF);
            if shm_offset != 0 {
                let overlay_info: &cartridge_io::FsOverlayInfoHeader = unsafe { mem::transmute(self.emu.mem.shm.as_ptr().add(shm_offset)) };
                if (overlay_info.id as usize) < self.emu.cartridge.io.overlays.len() {
                    let stored_overlay = &self.emu.cartridge.io.overlays[overlay_info.id as usize];
                    if overlay_info == stored_overlay {
                        if self.is_addr_fs_clear_overlay_image(guest_pc, thumb) {
                            self.emu.fs_clear_overlay_image_addr = guest_pc;
                        }
                    }
                }
            }
        }

        if self.emu.fs_clear_overlay_image_addr == 0 || self.emu.fs_clear_overlay_image_addr != guest_pc {
            return;
        }

        block_asm.call(fs_clear_overlay_image_hook as _);
        block_asm.is_fs_clear_overlay = true;
        info_println!("Found fs clear overlay at {guest_pc:x}");
    }

    pub fn emit_nitrosdk_func(&mut self, guest_pc: u32, thumb: bool) -> bool {
        if !self.emu.nitro_sdk_version.is_valid() || thumb {
            return false;
        }

        if self.emu.nitro_sdk_version.is_twl_sdk() && self.emu.settings.arm7_emu() == Arm7Emu::Hle && guest_pc & 0xFFFF000 == 0x1FF8000 {
            const MICROCODE_FUNCTIONS: &[Function] = &[
                Function::new(&MICROCODE_SHAKEHAND, "MICROCODE_SHAKEHAND", hle_microcode_shakehand),
                Function::new(&MICROCODE_WAIT_AGREEMENT, "MICROCODE_WAIT_AGREEMENT", hle_microcode_wait_agreement),
            ];

            for func in MICROCODE_FUNCTIONS {
                if func.eq(&self.jit_buf.insts) {
                    let pc_end = guest_pc + ((self.jit_buf.insts.len() as u32) << if thumb { 1 } else { 2 });
                    self.emu.jit_protect_region::<{ ARM9 }>(guest_pc, pc_end, thumb, &regions::ITCM_REGION);
                    self.emu.jit_set_live_range(guest_pc, pc_end, thumb);
                    unsafe {
                        *self.emu.jit.jit_memory_map.get_jit_entry(guest_pc) = JitEntry(func.hle_function as _);
                        (func.hle_function)(guest_pc);
                    }
                    return true;
                }
            }
        }

        let functions: &[Function] = match self.cpu {
            ARM9 => FUNCTIONS_ARM9,
            ARM7 => FUNCTIONS_ARM7,
        };

        for func in functions {
            if func.opcodes.len() == self.jit_buf.insts.len() {
                if func.eq(&self.jit_buf.insts) {
                    debug_println!("{:?} found {} at {guest_pc:x}", self.cpu, func.name);
                    unsafe {
                        *self.emu.jit.jit_memory_map.get_jit_entry(guest_pc) = JitEntry(func.hle_function as _);
                        (func.hle_function)(guest_pc);
                    }
                    return true;
                }
            } else if func.opcodes.len() > self.jit_buf.insts.len() {
                break;
            }
        }
        false
    }

    pub fn emit_hle_os_irq_handler(&mut self, guest_pc: u32, thumb: bool) -> bool {
        if thumb {
            return false;
        }

        let mut irq_table_addr = 0;
        let mut thread_switch_addr = 0;
        for (i, inst) in self.jit_buf.insts.iter().enumerate().rev() {
            let current_pc = guest_pc + ((i as u32) << 2);
            match (inst.op, inst.imm_transfer_addr(current_pc)) {
                (Op::Ldr(transfer), Some(imm_addr)) if transfer.size() == 2 => {
                    let imm_value = self.emu.mem_read::<{ ARM9 }, u32>(imm_addr);

                    if thread_switch_addr == 0 && inst.operands()[0].as_reg_no_shift().unwrap() == Reg::LR {
                        thread_switch_addr = imm_value;
                    } else if irq_table_addr == 0 {
                        irq_table_addr = imm_value;
                    }
                }
                _ => {}
            }

            if irq_table_addr != 0 && thread_switch_addr != 0 {
                break;
            }
        }

        if irq_table_addr == 0 || thread_switch_addr == 0 {
            return false;
        }

        self.emu.os_irq_table_addr = irq_table_addr;
        self.emu.os_irq_handler_thread_switch_addr = thread_switch_addr;

        unsafe {
            *self.emu.jit.jit_memory_map.get_jit_entry(guest_pc) = JitEntry(hle_os_irqhandler as _);
            hle_os_irqhandler(guest_pc);
        }

        true
    }
}

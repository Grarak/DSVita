use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::jit::inst_branch_handler::check_scheduler;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm::JitAsm;
use crate::jit::jit_asm_common_funs::exit_guest_context;
use crate::jit::jit_memory::JitEntry;
use crate::logging::debug_println;
use crate::settings::Arm7Emu;
use crate::{get_jit_asm_ptr, IS_DEBUG};
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

const FUNCTIONS_ARM9: &[Function] = &[
    Function::new(&MI_CPU_CLEAR32, "MI_CPU_CLEAR32", hle_mi_cpu_clear32::<{ ARM9 }>),
    Function::new(&MI_CPU_CLEAR16, "MI_CPU_CLEAR16", hle_mi_cpu_clear16::<{ ARM9 }>),
    Function::new(&MI_CPU_COPY32, "MI_CPU_COPY32", hle_mi_cpu_copy32::<{ ARM9 }>),
    Function::new(&MI_CPU_SEND32, "MI_CPU_SEND32", hle_mi_cpu_send32::<{ ARM9 }>),
    Function::new(&MI_CPU_COPY16, "MI_CPU_COPY16", hle_mi_cpu_copy16::<{ ARM9 }>),
    Function::new(&GX_FIFO_SEND64B, "GX_FIFO_SEND64B", hle_gx_fifo_send64b),
    Function::new(&GX_FIFO_SEND48B, "GX_FIFO_SEND48B", hle_gx_fifo_send48b),
    Function::new(&GX_FIFO_SEND128B, "GX_FIFO_SEND128B", hle_gx_fifo_send128b),
    Function::new(&MI_COPY64B, "MI_COPY64B", hle_mi_copy64b::<{ ARM9 }>),
    Function::new(&MI_CPU_CLEARFAST, "MI_CPU_CLEARFAST", hle_mi_cpu_clearfast::<{ ARM9 }>),
    Function::new(&MI_CPU_FILL8, "MI_CPU_FILL8", hle_mi_cpu_fill8::<{ ARM9 }>),
    Function::new(&GX_FIFO_NOP_CLEAR128, "GX_FIFO_NOP_CLEAR128", hle_gx_fifo_nop_clear128),
];

const FUNCTIONS_ARM7: &[Function] = &[
    Function::new(&MI_CPU_CLEAR32, "MI_CPU_CLEAR32", hle_mi_cpu_clear32::<{ ARM7 }>),
    Function::new(&MI_CPU_CLEAR16, "MI_CPU_CLEAR16", hle_mi_cpu_clear16::<{ ARM7 }>),
    Function::new(&MI_CPU_COPY32, "MI_CPU_COPY32", hle_mi_cpu_copy32::<{ ARM7 }>),
    Function::new(&MI_CPU_SEND32, "MI_CPU_SEND32", hle_mi_cpu_send32::<{ ARM7 }>),
    Function::new(&MI_CPU_COPY16, "MI_CPU_COPY16", hle_mi_cpu_copy16::<{ ARM7 }>),
    Function::new(&MI_COPY64B, "MI_COPY64B", hle_mi_copy64b::<{ ARM7 }>),
    Function::new(&MI_CPU_CLEARFAST, "MI_CPU_CLEARFAST", hle_mi_cpu_clearfast::<{ ARM7 }>),
    Function::new(&MI_CPU_FILL8, "MI_CPU_FILL8", hle_mi_cpu_fill8::<{ ARM7 }>),
];

impl JitAsm<'_> {
    pub fn emit_nitrosdk_func(&mut self, guest_pc: u32, thumb: bool) -> bool {
        if thumb {
            return false;
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
}

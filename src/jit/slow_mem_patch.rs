// aarch64 fast-mem slow-path patching. A SIGSEGV in an emitted fast-mem window (io region
// or a write into a jit-protected page) rewrites the window in place with a call to the
// shared inst_mem_handler cores, keyed by the faulting instruction's per-block metadata.
// Split out of jit_memory.rs — this and the multiple-transfer patch shapes are the growing
// part. The whole file is aarch64-only (the mod declaration is cfg-gated).

use super::jit_memory::JitMemory;
use crate::core::memory::io_arm7::io_arm7;
use crate::core::memory::io_arm9::io_arm9;
use crate::core::memory::regions;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::GuestInstMetadata;
use crate::jit::op::{MultipleTransfer, Op, SingleTransfer};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::MemoryAmount;
use crate::logging::debug_println;
use crate::mmap::{flush_icache, ArmContext, PAGE_SHIFT};
use std::hint::unreachable_unchecked;
use std::slice;

impl JitMemory {
    fn get_inst_mem_handler_fun_a64<const CPU: CpuType>(is_write: bool, transfer: SingleTransfer, guest_memory_addr: u32, io_func: &mut Option<*const ()>) -> *const () {
        use crate::jit::inst_mem_handler::{_inst_read_io_mem_handler, _inst_read_mem_handler, _inst_write_io_mem_handler, _inst_write_mem_handler};

        macro_rules! pick {
            ($write_func:ident, $read_func:ident) => {
                match (is_write, transfer.size()) {
                    (true, 0) => $write_func::<CPU, { MemoryAmount::Byte }> as _,
                    (true, 1) => $write_func::<CPU, { MemoryAmount::Half }> as _,
                    (true, 2) => $write_func::<CPU, { MemoryAmount::Word }> as _,
                    (false, 0) => {
                        if transfer.signed() {
                            $read_func::<CPU, { MemoryAmount::Byte }, true> as _
                        } else {
                            $read_func::<CPU, { MemoryAmount::Byte }, false> as _
                        }
                    }
                    (false, 1) => {
                        if transfer.signed() {
                            $read_func::<CPU, { MemoryAmount::Half }, true> as _
                        } else {
                            $read_func::<CPU, { MemoryAmount::Half }, false> as _
                        }
                    }
                    (false, 2) => $read_func::<CPU, { MemoryAmount::Word }, false> as _,
                    _ => unsafe { unreachable_unchecked() },
                }
            };
        }

        // GX fifo writes stay on the generic handler: handle_request_write runs the
        // fifo-full/scheduler semantics the raw io functions skip (arm32 routes this
        // range to its dedicated gxfifo handlers before any io specialization).
        if CPU == ARM9 && is_write && guest_memory_addr >= 0x4000400 && guest_memory_addr < 0x4000440 {
            return pick!(_inst_write_mem_handler, _inst_read_mem_handler);
        }

        if guest_memory_addr & 0xFF000000 == regions::IO_PORTS_OFFSET {
            let io_addr = guest_memory_addr & 0xFFFFFF;
            let dma_range = 0xB0..=0xEC;
            let spu_range = 0x400..=0x4FC;
            let size = 1u32 << transfer.size();
            let aligned_addr = io_addr & !(size - 1);
            *io_func = match CPU {
                ARM9 if !dma_range.contains(&io_addr) => {
                    if is_write {
                        io_arm9::get_write_with_size(aligned_addr, size as usize).map(|f| f as _)
                    } else {
                        io_arm9::get_read_with_size(aligned_addr, size as usize).map(|f| f as _)
                    }
                }
                ARM7 if !dma_range.contains(&io_addr) && !spu_range.contains(&io_addr) => {
                    if is_write {
                        io_arm7::get_write_with_size(aligned_addr, size as usize).map(|f| f as _)
                    } else {
                        io_arm7::get_read_with_size(aligned_addr, size as usize).map(|f| f as _)
                    }
                }
                _ => None,
            };
            if io_func.is_some() {
                return pick!(_inst_write_io_mem_handler, _inst_read_io_mem_handler);
            }
        }

        pick!(_inst_write_mem_handler, _inst_read_mem_handler)
    }

    /// The a64 ldm/stm handler monomorphization for an instruction's shape — emit-time
    /// selection (always-slow, no address specialization; the generic core routes io
    /// including gxfifo through handle_multiple_request).
    pub fn get_inst_mem_multiple_handler_fun_a64<const CPU: CpuType>(is_write: bool, transfer: MultipleTransfer, user: bool, valid: bool, needs_pc: bool) -> *const () {
        use crate::jit::inst_mem_handler::inst_mem_handler_multiple_a64 as h;
        debug_assert!(transfer.write_back() || valid);
        debug_assert!(is_write || !needs_pc);
        debug_assert!(!user || !needs_pc);
        macro_rules! pick {
            ($($w:literal, $wb:literal, $dec:literal, $v:literal, $u:literal, $pc:literal);+ $(;)?) => {
                match (is_write, transfer.write_back(), !transfer.add(), valid, user, needs_pc) {
                    $(($w, $wb, $dec, $v, $u, $pc) => h::<CPU, $w, $wb, $dec, $v, $u, $pc> as _,)+
                    _ => unsafe { unreachable_unchecked() },
                }
            };
        }
        pick!(
            false, false, false, true, false, false;
            false, false, true, true, false, false;
            false, true, false, true, false, false;
            false, true, true, true, false, false;
            false, false, false, true, true, false;
            false, false, true, true, true, false;
            false, true, false, true, true, false;
            false, true, true, true, true, false;
            // ldm with writeback and the base in the list (valid=false): hardware resolves
            // the conflict per-cpu (the handler's runtime edge conditions) — the loaded
            // value wins on ARM9, ARM7 differs. Same rows the arm32 table has.
            false, true, false, false, false, false;
            false, true, true, false, false, false;
            false, true, false, false, true, false;
            false, true, true, false, true, false;
            true, false, false, true, false, false;
            true, false, true, true, false, false;
            true, true, false, true, false, false;
            true, true, true, true, false, false;
            true, true, false, false, false, false;
            true, true, true, false, false, false;
            true, false, false, true, true, false;
            true, false, true, true, true, false;
            true, true, false, true, true, false;
            true, true, true, true, true, false;
            true, true, false, false, true, false;
            true, true, true, false, true, false;
            true, false, false, true, false, true;
            true, false, true, true, false, true;
            true, true, false, true, false, true;
            true, true, true, true, false, true;
            true, true, false, false, false, true;
            true, true, true, false, false, true;
        )
    }

    /// Rewrite a faulting a64 fast-mem window in place with the slow-path call. The
    /// canonical contract with emit_single_transfer: w11 holds the unaligned guest
    /// address, the mapped op0 the value.
    unsafe fn execute_patch_slow_mem_a64(host_pc: &mut usize, guest_memory_addr: u32, window_start: usize, window_size: usize, metadata: &mut GuestInstMetadata, cpu: CpuType) {
        use crate::jit::assembler::aarch64::encode;
        use crate::jit::inst_mem_handler::inst_write_breakout_shim_a64;
        use vixl::A64Reg;

        // Multi-register ldm/stm: rewrite the whole window to the shared multiple slow
        // handler (it dumps the pool, does the whole transfer incl. base writeback off guest
        // memory — the emitter cleared the mapping — and breaks out on a protected write).
        // A single-register multiple falls through to the word-transfer path below.
        if metadata.s.fast.op.is_multiple_mem_transfer() {
            let rlist = metadata.s.fast.operands.values[1].as_reg_list().unwrap_unchecked();
            if rlist.len() > 1 {
                Self::patch_multiple_slow_mem_a64(host_pc, window_start, window_size, metadata, rlist, cpu);
                return;
            }
        }

        let transfer = match metadata.s.fast.op {
            Op::Ldr(transfer) | Op::LdrT(transfer) | Op::Str(transfer) | Op::StrT(transfer) => transfer,
            // A single-register ldm/stm is a word transfer; the ±4 base writeback was folded
            // into the fast window's patch-immune pre-block, so the slow handler only does the
            // word access off the address in w11 (the pre/writeback flags here are unused).
            Op::Ldm(_) | Op::LdmT(_) | Op::Stm(_) | Op::StmT(_) => SingleTransfer::new(false, false, true, false, 2),
            _ => unreachable_unchecked(),
        };
        let is_write = metadata.s.fast.op.is_write_mem_transfer();
        let op0_mapped = metadata.s.fast.op0;

        let mut io_func = None;
        let handler = match cpu {
            ARM9 => Self::get_inst_mem_handler_fun_a64::<{ ARM9 }>(is_write, transfer, guest_memory_addr, &mut io_func),
            ARM7 => Self::get_inst_mem_handler_fun_a64::<{ ARM7 }>(is_write, transfer, guest_memory_addr, &mut io_func),
        };
        if let Some(io_func) = io_func {
            metadata.s.slow.initial_patch_addr = guest_memory_addr;
            metadata.s.slow.io_func = io_func;
        }
        let metadata_ptr = metadata as *const GuestInstMetadata as u64;

        let mut buf: Vec<u32> = Vec::with_capacity(window_size / 4);
        if is_write {
            // _inst_write[_io]_mem_handler(value0: w0, value1: w1, addr: w2, metadata: x3)
            // -> metadata-or-null; non-null means the write invalidated the running
            // block — hand the pool dump to the breakout shim (never returns).
            buf.push(encode::mov_reg(A64Reg::X0, op0_mapped));
            buf.push(encode::mov_reg(A64Reg::X2, A64Reg::X11));
            encode::emit_mov_imm64(&mut buf, A64Reg::X3, metadata_ptr);
            encode::emit_mov_imm64(&mut buf, A64Reg::X8, handler as u64);
            buf.push(encode::blr(A64Reg::X8));
            buf.push(encode::cbz64(A64Reg::X0, 6));
            encode::emit_mov_imm64(&mut buf, A64Reg::X8, inst_write_breakout_shim_a64 as *const () as u64);
            buf.push(encode::blr(A64Reg::X8));
        } else {
            // _inst_read[_io]_mem_handler(metadata-or-unused: x0, _: x1, addr: w2) -> w0,
            // already rotated for unaligned words.
            if io_func.is_some() {
                encode::emit_mov_imm64(&mut buf, A64Reg::X0, metadata_ptr);
            }
            buf.push(encode::mov_reg(A64Reg::X2, A64Reg::X11));
            encode::emit_mov_imm64(&mut buf, A64Reg::X8, handler as u64);
            buf.push(encode::blr(A64Reg::X8));
            buf.push(encode::mov_reg(op0_mapped, A64Reg::X0));
        }
        while buf.len() * 4 < window_size {
            buf.push(encode::NOP);
        }
        debug_assert!(buf.len() * 4 == window_size);

        let window = slice::from_raw_parts_mut(window_start as *mut u8, window_size);
        for (i, inst) in buf.iter().enumerate() {
            window[i * 4..i * 4 + 4].copy_from_slice(&inst.to_le_bytes());
        }
        *host_pc = window_start;
        flush_icache(window.as_ptr(), window.len());
    }

    /// Rewrite a faulting multi-register ldm/stm window with the shared multiple slow
    /// handler call. The window (base-address setup, the unrolled LDP/STP accesses, and the
    /// base writeback) is entirely discarded — the handler re-derives everything from guest
    /// memory (the emitter cleared the mapping) and applies the writeback itself, so the
    /// discarded in-window writeback never double-applies.
    unsafe fn patch_multiple_slow_mem_a64(host_pc: &mut usize, window_start: usize, window_size: usize, metadata: &GuestInstMetadata, rlist: RegReserve, cpu: CpuType) {
        use crate::jit::assembler::aarch64::encode;
        use crate::jit::inst_mem_handler::InstMemMultipleParams;
        use bilge::arbitrary_int::{u4, u6};
        use vixl::A64Reg;

        let op = metadata.s.fast.op;
        let transfer = match op {
            Op::Ldm(t) | Op::LdmT(t) | Op::Stm(t) | Op::StmT(t) => t,
            _ => unreachable_unchecked(),
        };
        let is_write = op.is_write_mem_transfer();
        let op0 = metadata.s.fast.operands.values[0].as_reg_no_shift().unwrap_unchecked();
        let user = transfer.user() && !rlist.is_reserved(Reg::PC);
        let valid = !transfer.write_back() || !rlist.is_reserved(op0);
        // A stored pc reads the pipeline value — the guest PC slot is stale mid-block
        // (the fast window materializes it the same way).
        let needs_pc = is_write && rlist.is_reserved(Reg::PC);
        let mut pre = transfer.pre();
        if !transfer.add() {
            pre = !pre;
        }
        let params = InstMemMultipleParams::new(rlist.0 as u16, u4::new(rlist.len() as u8), u4::new(op0 as u8), pre, transfer.user(), u6::new(0));
        let handler = match cpu {
            ARM9 => Self::get_inst_mem_multiple_handler_fun_a64::<{ ARM9 }>(is_write, transfer, user, valid, needs_pc),
            ARM7 => Self::get_inst_mem_multiple_handler_fun_a64::<{ ARM7 }>(is_write, transfer, user, valid, needs_pc),
        };
        let metadata_ptr = metadata as *const GuestInstMetadata as u64;

        // inst_mem_handler_multiple_a64(params: w0, metadata: x1) — the naked wrapper dumps
        // the pool itself; it breaks out on a protected write, else returns (io) and the
        // window's trailing nops fall through to the next instruction.
        let mut buf: Vec<u32> = Vec::with_capacity(window_size / 4);
        encode::emit_mov_imm64(&mut buf, A64Reg::X0, u32::from(params) as u64);
        encode::emit_mov_imm64(&mut buf, A64Reg::X1, metadata_ptr);
        encode::emit_mov_imm64(&mut buf, A64Reg::X8, handler as u64);
        buf.push(encode::blr(A64Reg::X8));
        while buf.len() * 4 < window_size {
            buf.push(encode::NOP);
        }
        debug_assert!(buf.len() * 4 == window_size);

        let window = slice::from_raw_parts_mut(window_start as *mut u8, window_size);
        for (i, inst) in buf.iter().enumerate() {
            window[i * 4..i * 4 + 4].copy_from_slice(&inst.to_le_bytes());
        }
        *host_pc = window_start;
        flush_icache(window.as_ptr(), window.len());
    }

    pub unsafe fn patch_slow_mem(&mut self, host_pc: &mut usize, guest_memory_addr: u32, cpu: CpuType, _: &ArmContext) -> bool {
        if !self.is_in_jit_mem(*host_pc) {
            eprintln!("Segfault outside of guest context (pc {:x}, jit {:x}+{:x})", *host_pc, self.mem.as_ptr() as usize, self.mem.len());
            return false;
        }

        let jit_mem_offset = *host_pc - self.mem.as_ptr() as usize;
        let base_page = self.a64_page_owner[jit_mem_offset >> PAGE_SHIFT] as usize;
        let block_base = self.mem.as_ptr() as usize + (base_page << PAGE_SHIFT);
        let offset_in_block = (*host_pc - block_base) as u32;
        let meta = &mut self.a64_block_meta[base_page];
        let index = meta.metadatas.iter().position(|(offset, _)| *offset == offset_in_block);
        let Some(index) = index else {
            eprintln!("{cpu:?} fault at {host_pc:x} (block +{offset_in_block:x}) without fast-mem metadata");
            return false;
        };
        let (_, metadata) = &mut meta.metadatas[index];

        debug_println!("{cpu:?} slow mem patch at {:x} {:?} addr {guest_memory_addr:x}", metadata.pc, metadata.s.fast.op);

        let window_start = *host_pc - metadata.s.fast.start_offset as usize;
        let window_size = metadata.s.fast.size as usize;
        Self::execute_patch_slow_mem_a64(host_pc, guest_memory_addr, window_start, window_size, metadata, cpu);
        true
    }
}

// The aarch64 code-generation backend, stage-5 slices 1-3 (see todo/aarch64-port.md D7).
//
// Runtime contract of an emitted block: a normal AAPCS64 function taking the tagged guest
// pc in w0. The prologue pins the cpu's ThreadRegs in x27 (callee-saved: the previous
// value is preserved in the frame, so blocks nest across the branch runtime like
// interpreter frames do) and then dispatches on w0 — entry at the block's start pc falls
// through into the body; any other pc inside the compiled range resolves through the
// jump-to-other-guest-pc runtime into the matching instruction offset (interrupt returns
// and hot mid-block branch targets land there). Guest registers live at [x27, #reg*4];
// every instruction body loads its sources fresh and stores its result back — no
// cross-instruction allocation yet, correctness first (the reg_alloc port is a later
// slice).
//
// Control flow (slice 3): local branches jump between instruction labels inside the
// block; external branches flush through the shared pre_branch runtime and then TAIL-CALL
// the target's jit entry — the block's frame is popped before the jump, so compiled
// block-to-block transfers no longer grow the host stack (the arm32 restore_stack + bx
// scheme). An emitted block's own `ret` is therefore never executed; control returns to
// the block's caller through whatever the chain eventually rets from.

use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::jit::assembler::aarch64::reg_alloc::A64RegAlloc;
use crate::jit::assembler::{GuestInstMetadata, GUEST_REGS_LENGTH, GUEST_REG_POOL_SIZE};
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use vixl::{A64AddrModeKind, A64Label, A64MacroAssembler, A64Reg, A64ShiftKind};

// Scratch w-registers for instruction bodies (caller-saved, outside vixl's x16/x17 and the
// argument registers used when calling into the runtime).
pub const SCRATCH0: A64Reg = A64Reg::X9;
pub const SCRATCH1: A64Reg = A64Reg::X10;
pub const SCRATCH2: A64Reg = A64Reg::X11;

/// ThreadRegs base register (D7): callee-saved, so runtime calls emitted mid-block keep it.
pub const GUEST_REGS_PTR: A64Reg = A64Reg::X27;

pub struct BlockAsm {
    pub masm: A64MacroAssembler,
    pub thumb: bool,
    pub is_os_irq_handler: bool,
    cpu: CpuType,
    /// Bound at buffer offset 0: `adr` against it materializes the block's host base
    /// address (blocks are page-aligned in jit memory, and the mid-entry runtime keys its
    /// per-block metadata by that base).
    start_label: A64Label,
    /// Host code offset of each guest instruction's first emitted byte, in instruction
    /// order — the mid-entry dispatch targets. Harvested by jit_insert_block_a64.
    pub inst_offsets: Vec<u32>,
    /// pre_cycle_count_sum of each instruction, recorded alongside inst_offsets so
    /// jit_insert_block can build the mid-entry metadata without the jit_buf.
    pub inst_pre_sums: Vec<u16>,
    /// The guest→host mapping in force at each instruction's entry (snapshotted next to
    /// inst_offsets) — the mid-entry dispatch restores it through the per-inst pointer
    /// table in A64BlockMeta.
    pub inst_mappings: Vec<[A64Reg; GUEST_REGS_LENGTH]>,
    /// Straight-line register allocator: mappings live between control-flow
    /// discontinuities; dirty registers get written back at every flush point.
    pub reg_alloc: A64RegAlloc,
    pub dirty_guest_regs: RegReserve,
    /// Per-basic-block jump labels (Some when the block is a local-branch entry), and the
    /// precomputed entry mapping (dirty&output regs, free-pool mask, guest→host mapping)
    /// each block is relocated to on entry — arm32's basic-block emitter twins.
    pub guest_basic_block_labels: Vec<Option<A64Label>>,
    basic_blocks_guest_regs_mappings: Vec<(RegReserve, u16, [A64Reg; GUEST_REGS_LENGTH])>,
    /// Patchable fast-mem windows and always-slow handler calls: (key offset, metadata),
    /// in emission order. Boxed so the metadata address is stable from emission on —
    /// the ldm/stm handler call bakes the pointer in at emit time (the singles patcher
    /// only needs it at fault time, but shares the storage).
    pub guest_inst_metadata: Vec<(u32, Box<GuestInstMetadata>)>,
}

impl BlockAsm {
    pub fn new(cpu: CpuType, thumb: bool, is_os_irq_handler: bool) -> Self {
        BlockAsm {
            masm: A64MacroAssembler::new(),
            thumb,
            is_os_irq_handler,
            cpu,
            start_label: A64Label::new(),
            inst_offsets: Vec::new(),
            inst_pre_sums: Vec::new(),
            inst_mappings: Vec::new(),
            reg_alloc: A64RegAlloc::new(),
            dirty_guest_regs: RegReserve::new(),
            guest_basic_block_labels: Vec::new(),
            basic_blocks_guest_regs_mappings: Vec::new(),
            guest_inst_metadata: Vec::new(),
        }
    }

    /// Emit the block frame and pin the ThreadRegs base (arm32's prologue twin). Sizes the
    /// per-basic-block label + entry-mapping tables the basic-block emitter fills in.
    pub fn prologue(&mut self, basic_block_count: usize) {
        self.guest_basic_block_labels.resize_with(basic_block_count, || None);
        self.basic_blocks_guest_regs_mappings
            .resize_with(basic_block_count, || (RegReserve::new(), (1 << GUEST_REG_POOL_SIZE) - 1, [A64Reg::ZR; GUEST_REGS_LENGTH]));
        let asm = self;
        let regs: *mut ThreadRegs = asm.cpu.thread_regs();
        asm.masm.bind(&mut asm.start_label);
        // Frame: fp/lr, then every callee-saved register the block can touch — x27 (the
        // ThreadRegs base) and the full x19-x26+x28 allocator pool. Blocks return to
        // Rust callers (call_jit_fun) that rely on them (arm32's prologue pushes r4-r11
        // identically). Tail chains stay stack-neutral: restore_frame pops it all first.
        asm.masm.stp(A64Reg::X29, A64Reg::X30, true, A64Reg::SP, -112, A64AddrModeKind::PreIndex);
        asm.masm.stp(GUEST_REGS_PTR, A64Reg::X28, true, A64Reg::SP, 16, A64AddrModeKind::Offset);
        asm.masm.stp(A64Reg::X19, A64Reg::X20, true, A64Reg::SP, 32, A64AddrModeKind::Offset);
        asm.masm.stp(A64Reg::X21, A64Reg::X22, true, A64Reg::SP, 48, A64AddrModeKind::Offset);
        asm.masm.stp(A64Reg::X23, A64Reg::X24, true, A64Reg::SP, 64, A64AddrModeKind::Offset);
        asm.masm.stp(A64Reg::X25, A64Reg::X26, true, A64Reg::SP, 80, A64AddrModeKind::Offset);
        asm.masm.mov_imm64(GUEST_REGS_PTR, regs as u64);
    }

    /// Entry-pc dispatch (the arm32 seam method's twin): entry at the block's start pc
    /// falls through; any other pc resolves through `jump_fun` (which returns the host
    /// address of the matching instruction and restores pre_cycle_count_sum) and jumps
    /// there. x0 carries the tagged pc into the helper unchanged.
    pub fn emit_entry_pc_dispatch(&mut self, tagged_guest_pc: u32, jump_fun: *const ()) {
        let mut body = A64Label::new();
        self.masm.mov_imm32(SCRATCH0, tagged_guest_pc);
        self.masm.cmp_reg(A64Reg::X0, SCRATCH0, A64ShiftKind::LSL, 0, false);
        self.masm.b_cond(&mut body, crate::jit::Cond::EQ);
        self.masm.adr(A64Reg::X1, &mut self.start_label);
        self.call_host(jump_fun);
        // x0 = target host address, x1 = the instruction's pool restore table: reload
        // every pool register through its slot pointer (unmapped ones point at a dummy
        // slot — free registers' contents are don't-care), then jump in.
        for (i, &pool_reg) in crate::jit::assembler::aarch64::reg_alloc::GUEST_REG_ALLOCATIONS.iter().enumerate() {
            self.masm.ldr_off(SCRATCH0, true, A64Reg::X1, (i * 8) as i64, A64AddrModeKind::Offset);
            self.masm.ldr_off(pool_reg, false, SCRATCH0, 0, A64AddrModeKind::Offset);
        }
        self.masm.br(A64Reg::X0);
        self.masm.bind(&mut body);
    }

    /// ARM7 homebrew self-modifying-code guard (arm32's emit_validate_block_hash):
    /// rehash the guest code at every fresh block entry; on a mismatch the helper
    /// recompiles and returns the new entry — pop our frame and tail-jump into it.
    /// x19 parks the tagged entry pc across the call (frame-saved, and the allocator
    /// holds nothing this early).
    pub fn emit_validate_block_hash(&mut self, guest_ptr: usize, size: u32, hash: u32, tagged_pc: u32, validate_fun: *const ()) {
        self.masm.mov_reg(A64Reg::X19, A64Reg::X0, true);
        self.masm.mov_imm64(A64Reg::X0, guest_ptr as u64);
        self.mov_imm(A64Reg::X1, size);
        self.mov_imm(A64Reg::X2, hash);
        self.mov_imm(A64Reg::X3, tagged_pc);
        self.call_host(validate_fun);
        let mut valid = A64Label::new();
        self.masm.cbz(A64Reg::X0, true, &mut valid);
        self.masm.mov_reg(A64Reg::X8, A64Reg::X0, true);
        self.restore_frame();
        self.masm.mov_imm32(A64Reg::X0, tagged_pc);
        self.masm.br(A64Reg::X8);
        self.masm.bind(&mut valid);
        self.masm.mov_reg(A64Reg::X0, A64Reg::X19, true);
    }

    /// BRANCH_LOG enter-block hook; x19 parks the entry pc around the call (frame-saved,
    /// allocator empty this early — arm32's r4 dance).
    pub fn emit_enter_block_hook(&mut self, hook_fun: *const ()) {
        self.masm.mov_reg(A64Reg::X19, A64Reg::X0, true);
        self.call_host(hook_fun);
        self.masm.mov_reg(A64Reg::X0, A64Reg::X19, true);
    }

    /// Record the host offset of the guest instruction about to be emitted, along with
    /// the register mapping in force there (the mid-entry restore needs both).
    pub fn record_inst_offset(&mut self, pre_cycle_count_sum: u16) {
        let offset = self.masm.get_cursor_offset();
        self.inst_offsets.push(offset);
        self.inst_pre_sums.push(pre_cycle_count_sum);
        self.inst_mappings.push(self.reg_alloc.guest_regs_mapping);
    }

    /// Allocate an instruction's registers (arm32's alloc_guest_inst shape): conditional
    /// instructions preload their outputs too, so a skipped body leaves the correct old
    /// values in the mapped registers and the later writeback stays sound. PC and CPSR
    /// stay memory-resident on this backend.
    pub fn alloc_guest_inst(&mut self, inst: &InstInfo, next_live_regs: RegReserve) {
        let mut input_regs = inst.src_regs;
        let output_regs = inst.out_regs - Reg::PC - Reg::CPSR;
        if inst.cond != Cond::AL {
            input_regs += output_regs;
        }
        let input_regs = (input_regs - Reg::PC - Reg::CPSR) - reg_reserve!(Reg::SPSR);
        let spilled = self.reg_alloc.alloc_guest_regs(input_regs, output_regs, next_live_regs, self.dirty_guest_regs, &mut self.masm);
        self.dirty_guest_regs -= spilled;
        self.dirty_guest_regs += output_regs;
    }

    /// The host register holding an allocated guest register.
    pub fn guest_map(&self, reg: Reg) -> A64Reg {
        self.reg_alloc.get_guest_map(reg)
    }

    /// Write dirty mapped registers back to guest memory (mappings stay live, dirty
    /// flags clear — the values match memory now). The conditional taken paths use this
    /// so the fall-through keeps its registers while the branch machinery sees coherent
    /// memory; the per-inst debug hook uses it so traces read current values.
    pub fn save_dirty_guest_regs(&mut self) {
        self.reg_alloc.save_dirty_guest_regs(self.dirty_guest_regs, &mut self.masm);
        self.dirty_guest_regs = RegReserve::new();
    }

    /// Reload every mapped register of `mapping` from guest memory — the conditional
    /// linking branches rejoin the fall-through with this after their call returned
    /// (the callee may have changed any guest register).
    pub fn reload_mapping(&mut self, mapping: &[A64Reg; GUEST_REGS_LENGTH]) {
        for (guest, &host) in mapping.iter().enumerate() {
            if host != A64Reg::ZR {
                self.masm.ldr_off(host, false, GUEST_REGS_PTR, guest as i64 * 4, A64AddrModeKind::Offset);
            }
        }
    }

    /// Flush point: write dirty registers back and drop every mapping. Emitted at every
    /// control-flow discontinuity (branch instructions, local branch targets), so jumps
    /// always arrive with coherent memory and rebuild mappings on demand.
    pub fn flush_guest_regs(&mut self) {
        self.reg_alloc.save_dirty_guest_regs(self.dirty_guest_regs, &mut self.masm);
        self.reg_alloc.clear();
        self.dirty_guest_regs = RegReserve::new();
    }

    /// Write a compile-time constant to a guest register through the mapping when one
    /// exists (BlSetupT's LR write — its decoder declares no outputs, so the allocator
    /// can't know; going around a live mapping would leave it stale).
    pub fn set_guest_imm(&mut self, guest: Reg, imm: u32) {
        let mapped = self.reg_alloc.guest_regs_mapping[guest as usize];
        if mapped != A64Reg::ZR {
            self.mov_imm(mapped, imm);
            self.dirty_guest_regs += guest;
        } else {
            self.mov_imm(SCRATCH2, imm);
            self.store_guest(SCRATCH2, guest);
        }
    }

    /// Mark a guest register's mapped value as newer than memory (transfer writebacks
    /// and loads mutate mapped registers outside alloc_guest_inst's bookkeeping).
    pub fn mark_guest_dirty(&mut self, guest: Reg) {
        self.dirty_guest_regs += guest;
    }

    /// Record a patchable fast-mem window (arm32's guest_inst_metadata): called
    /// immediately before emitting the access that can fault — the record is keyed by
    /// that instruction's exact host offset, and start_offset points back to the
    /// window's first byte. The metadata carries everything the SIGSEGV patcher needs
    /// to rewrite the window in place.
    pub fn guest_inst_metadata(&mut self, total_cycle_count: u16, inst: &InstInfo, fast_mem_start: u32, op0_mapped: A64Reg, tagged_pc: u32) {
        let offset = self.masm.get_cursor_offset();
        let mut dirty_guest_regs = self.dirty_guest_regs;
        for guest_reg in dirty_guest_regs - Reg::CPSR {
            if self.reg_alloc.guest_regs_mapping[guest_reg as usize] == A64Reg::ZR {
                dirty_guest_regs -= guest_reg;
            }
        }
        self.guest_inst_metadata.push((
            offset,
            Box::new(GuestInstMetadata::new(
                (offset - fast_mem_start) as u16,
                0,
                0,
                self.is_os_irq_handler,
                tagged_pc,
                total_cycle_count,
                inst.op,
                inst.operands,
                op0_mapped,
                dirty_guest_regs,
                self.reg_alloc.guest_regs_mapping,
            )),
        ));
    }

    /// The metadata just recorded, as a stable pointer (Boxed) for emit-time baking.
    pub fn last_guest_inst_metadata(&self) -> *const GuestInstMetadata {
        &*self.guest_inst_metadata.last().unwrap().1
    }

    /// Fix up the window size of the most recent guest_inst_metadata record.
    pub fn set_fast_mem_size_last(&mut self, size: u16) {
        self.guest_inst_metadata.last_mut().unwrap().1.s.fast.size = size;
    }

    pub fn get_guest_inst_metadata_len(&self) -> usize {
        self.guest_inst_metadata.len()
    }

    /// Set the window size on every guest_inst_metadata record from `start` on — a
    /// multi-register transfer records one per fault-site access, all pointing at the same
    /// window (arm32's set_fast_mem_size).
    pub fn set_fast_mem_size(&mut self, start: usize, size: u16) {
        for (_, metadata) in &mut self.guest_inst_metadata[start..] {
            metadata.s.fast.size = size;
        }
    }

    /// Compile-time-only variant for paths where the spills were already emitted on
    /// every arriving path (unreachable fall-throughs after unconditional branches).
    pub fn clear_guest_regs_mapping(&mut self) {
        self.reg_alloc.clear();
        self.dirty_guest_regs = RegReserve::new();
    }

    // --- Basic-block emitter seam (arm32 block_asm twins) ---
    // A block is a straight-line run between control-flow discontinuities. The pre-pass
    // computes each block's canonical entry mapping (init_guest_regs_mapping); emission
    // installs it (init_guest_regs), binds the block's label if it's a branch target
    // (bind_basic_block), and every edge into a block relocates the live mapping to that
    // canonical mapping (relocate_for_basic_block) before the jump. Guest PC and CPSR stay
    // memory-resident on a64, so unlike arm32 there is no cross-block cpsr-in-host dance.

    /// Pre-pass: reserve host homes for a block's live-in guest regs (first-fit, no code
    /// emitted) and record the resulting entry mapping. `output_regs` is the block's
    /// written set — the recorded dirty subset is inputs that the block also writes.
    pub fn init_guest_regs_mapping(&mut self, guest_regs: RegReserve, output_regs: RegReserve, basic_block_index: usize) {
        self.reg_alloc = A64RegAlloc::new();
        let dirty_guest_regs = self.reg_alloc.reserve_guest_regs(guest_regs - Reg::CPSR - Reg::PC, false, &mut self.masm);
        self.basic_blocks_guest_regs_mappings[basic_block_index] = (dirty_guest_regs & output_regs, self.reg_alloc.free_regs(), self.reg_alloc.guest_regs_mapping);
    }

    /// Emission: install block `basic_block_index`'s precomputed entry mapping. Block 0's
    /// input values still have to be loaded from memory (reload_active_guest_regs_all in
    /// the driver); later blocks receive them relocated from their predecessor.
    pub fn init_guest_regs(&mut self, basic_block_index: usize) {
        let (dirty_guest_regs, free_regs, guest_regs_mapping) = self.basic_blocks_guest_regs_mappings[basic_block_index];
        self.dirty_guest_regs = dirty_guest_regs;
        self.reg_alloc.set_guest_regs_mappings(&guest_regs_mapping);
        self.reg_alloc.set_free_regs(free_regs);
    }

    /// Reload every mapped guest register of the current mapping from memory — block 0's
    /// entry (values are still only in guest memory there).
    pub fn reload_active_guest_regs_all(&mut self) {
        self.reg_alloc.reload_active_guest_regs(RegReserve::all() - Reg::PC, &mut self.masm);
    }

    pub fn bind_basic_block(&mut self, basic_block_index: usize) {
        self.masm.bind(self.guest_basic_block_labels[basic_block_index].as_mut().unwrap());
    }

    pub fn b_basic_block(&mut self, basic_block_index: usize) {
        self.masm.b(self.guest_basic_block_labels[basic_block_index].as_mut().unwrap());
    }

    /// Shuffle the live mapping into block `basic_block_index`'s canonical entry mapping
    /// before jumping to it (arm32's relocate_for_basic_block).
    pub fn relocate_for_basic_block(&mut self, basic_block_output_regs: RegReserve, basic_block_index: usize) {
        let desired_mapping = self.basic_blocks_guest_regs_mappings[basic_block_index].2;
        self.reg_alloc
            .relocate_guest_regs(self.dirty_guest_regs - Reg::PC - Reg::CPSR, basic_block_output_regs, &desired_mapping, &mut self.masm);
    }

    /// Enter a basic block whose registers must be (re)loaded from guest memory rather than
    /// relocated from a predecessor — the idle-loop reset, where the scheduler/interrupt
    /// excursion left the pool stale (arm32's init_basic_block_regs). CPSR stays memory-
    /// resident on a64, so there is no host-flag reload.
    pub fn init_basic_block_regs(&mut self, basic_block_index: usize) {
        let mapping = self.basic_blocks_guest_regs_mappings[basic_block_index].2;
        self.reg_alloc.set_guest_regs_mappings(&mapping);
        self.reload_active_guest_regs_all();
    }

    pub fn set_guest_regs_mapping(&mut self, guest_regs_mapping: [A64Reg; GUEST_REGS_LENGTH]) {
        self.reg_alloc.set_guest_regs_mappings(&guest_regs_mapping);
    }

    pub fn get_guest_regs_mapping(&self) -> [A64Reg; GUEST_REGS_LENGTH] {
        self.reg_alloc.guest_regs_mapping
    }

    pub fn add_dirty_guest_regs(&mut self, guest_regs: RegReserve) {
        self.dirty_guest_regs += guest_regs;
    }

    /// Materialize the block's own host base address (== its jit entry: blocks are
    /// page-aligned and untagged in ARM mode) — the forward-branch validity check
    /// compares it against the entry slot.
    pub fn adr_start(&mut self, dst: A64Reg) {
        self.masm.adr(dst, &mut self.start_label);
    }

    /// Guest register slot offset: gp_regs..spsr are one contiguous repr(C) block of u32s
    /// indexed like the Reg enum — the same layout trick the interpreter uses.
    fn guest_offset(reg: Reg) -> i64 {
        debug_assert!(reg as u32 <= Reg::SPSR as u32);
        reg as i64 * 4
    }

    pub fn load_guest(&mut self, dst: A64Reg, guest: Reg) {
        self.masm.ldr_off(dst, false, GUEST_REGS_PTR, Self::guest_offset(guest), A64AddrModeKind::Offset);
    }

    pub fn store_guest(&mut self, src: A64Reg, guest: Reg) {
        self.masm.str_off(src, false, GUEST_REGS_PTR, Self::guest_offset(guest), A64AddrModeKind::Offset);
    }

    pub fn mov_imm(&mut self, dst: A64Reg, imm: u32) {
        self.masm.mov_imm32(dst, imm);
    }

    /// Call a host function; w0-w3 arguments are set by the caller beforehand. x27 survives
    /// (callee-saved); every scratch register does not.
    pub fn call_host(&mut self, fun: *const ()) {
        self.masm.mov_imm64(A64Reg::X8, fun as u64);
        self.masm.blr(A64Reg::X8);
    }

    /// Pop the block's own frame (caller's x27, fp/lr) without leaving the function — the
    /// tail-call exits jump onward from here with the stack exactly as on entry.
    pub fn restore_frame(&mut self) {
        self.masm.ldp(GUEST_REGS_PTR, A64Reg::X28, true, A64Reg::SP, 16, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X19, A64Reg::X20, true, A64Reg::SP, 32, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X21, A64Reg::X22, true, A64Reg::SP, 48, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X23, A64Reg::X24, true, A64Reg::SP, 64, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X25, A64Reg::X26, true, A64Reg::SP, 80, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X29, A64Reg::X30, true, A64Reg::SP, 112, A64AddrModeKind::PostIndex);
    }

    /// Emitted twin of the exit_guest_context! macro: reset sp to the guest-context entry
    /// frame and pop call_jit_entry's callee-saved layout. Never returns to the block.
    pub fn emit_exit_guest_context(&mut self, host_sp_ptr: *mut usize) {
        self.masm.mov_imm64(A64Reg::X8, host_sp_ptr as u64);
        self.masm.ldr_off(A64Reg::X8, true, A64Reg::X8, 0, A64AddrModeKind::Offset);
        self.masm.mov_reg(A64Reg::SP, A64Reg::X8, true);
        self.masm.ldp(A64Reg::X19, A64Reg::X20, true, A64Reg::SP, 16, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X21, A64Reg::X22, true, A64Reg::SP, 32, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X23, A64Reg::X24, true, A64Reg::SP, 48, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X25, A64Reg::X26, true, A64Reg::SP, 64, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X27, A64Reg::X28, true, A64Reg::SP, 80, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X29, A64Reg::X30, true, A64Reg::SP, 96, A64AddrModeKind::PostIndex);
        self.masm.ret();
    }

    pub fn lsl_imm(&mut self, dst: A64Reg, src: A64Reg, amount: u32) {
        self.masm.lsl_imm(dst, src, amount, false);
    }

    pub fn shift_reg_imm(&mut self, dst: A64Reg, src: A64Reg, kind: A64ShiftKind, amount: u32) {
        match kind {
            A64ShiftKind::LSL => self.masm.lsl_imm(dst, src, amount, false),
            A64ShiftKind::LSR => self.masm.lsr_imm(dst, src, amount, false),
            A64ShiftKind::ASR => self.masm.asr_imm(dst, src, amount, false),
            A64ShiftKind::ROR => self.masm.ror_imm(dst, src, amount, false),
        }
    }

    /// Guest CPSR flag helpers for the flags-in-memory strategy: S-ops merge fresh NZCV
    /// bits into the stored cpsr word; conditional instructions load it into host NZCV.
    pub fn load_cpsr(&mut self, dst: A64Reg) {
        self.load_guest(dst, Reg::CPSR);
    }

    pub fn store_cpsr(&mut self, src: A64Reg) {
        self.store_guest(src, Reg::CPSR);
    }

    pub fn finalize(&mut self) {
        self.masm.finalize();
    }

    pub fn get_code_buffer(&self) -> &[u8] {
        self.masm.get_code_buffer()
    }
}

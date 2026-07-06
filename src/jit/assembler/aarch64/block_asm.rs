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
use crate::jit::reg::Reg;
use vixl::{A64AddrModeKind, A64Label, A64MacroAssembler, A64Reg, A64ShiftKind};

// Scratch w-registers for instruction bodies (caller-saved, outside vixl's x16/x17 and the
// argument registers used when calling into the runtime).
pub const SCRATCH0: A64Reg = A64Reg::X9;
pub const SCRATCH1: A64Reg = A64Reg::X10;
pub const SCRATCH2: A64Reg = A64Reg::X11;

/// ThreadRegs base register (D7): callee-saved, so runtime calls emitted mid-block keep it.
pub const GUEST_REGS_PTR: A64Reg = A64Reg::X27;

pub struct A64BlockAsm {
    pub masm: A64MacroAssembler,
    pub thumb: bool,
    /// Bound at buffer offset 0: `adr` against it materializes the block's host base
    /// address (blocks are page-aligned in jit memory, and the mid-entry runtime keys its
    /// per-block metadata by that base).
    start_label: A64Label,
    /// Host code offset of each guest instruction's first emitted byte, in instruction
    /// order — the mid-entry dispatch targets. Harvested by jit_insert_block_a64.
    pub inst_offsets: Vec<u32>,
}

impl A64BlockAsm {
    pub fn new(cpu: CpuType, thumb: bool) -> Self {
        let mut asm = A64BlockAsm {
            masm: A64MacroAssembler::new(),
            thumb,
            start_label: A64Label::new(),
            inst_offsets: Vec::new(),
        };
        let regs: *mut ThreadRegs = cpu.thread_regs();
        asm.masm.bind(&mut asm.start_label);
        // Frame: fp/lr + the caller's x27; then pin ThreadRegs.
        asm.masm.stp(A64Reg::X29, A64Reg::X30, true, A64Reg::SP, -32, A64AddrModeKind::PreIndex);
        asm.masm.str_off(GUEST_REGS_PTR, true, A64Reg::SP, 16, A64AddrModeKind::Offset);
        asm.masm.mov_imm64(GUEST_REGS_PTR, regs as u64);
        asm
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
        self.masm.br(A64Reg::X0);
        self.masm.bind(&mut body);
    }

    /// Record the host offset of the guest instruction about to be emitted.
    pub fn record_inst_offset(&mut self) {
        let offset = self.masm.get_cursor_offset();
        self.inst_offsets.push(offset);
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
        self.masm.ldr_off(GUEST_REGS_PTR, true, A64Reg::SP, 16, A64AddrModeKind::Offset);
        self.masm.ldp(A64Reg::X29, A64Reg::X30, true, A64Reg::SP, 32, A64AddrModeKind::PostIndex);
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

    pub fn finalize(&mut self) -> &[u8] {
        self.masm.finalize();
        self.masm.get_code_buffer()
    }
}

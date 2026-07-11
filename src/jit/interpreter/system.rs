// Software interrupts and cp15 coprocessor transfers. Semantics mirror the jit's runtime
// handlers (software_interrupt_handler / emit_cp15): the hle bios executes the swi inline,
// cp15 goes through cp15_read/cp15_write with the wait-for-irq registers halting the cpu.
// A halt requests an immediate breakout through the same flag the io write handlers use.

use super::{Ctx, InstResult, T_BIT};
use crate::core::exception_handler::{self, ExceptionVector};
use crate::core::CpuType::{ARM7, ARM9};

fn swi_common(ctx: &mut Ctx, comment: u8, step: u32) -> InstResult {
    let cpu = ctx.cpu;
    let thumb = ctx.cpsr() & T_BIT != 0;
    let resume = (ctx.inst_addr + step) | thumb as u32;
    unsafe { (*ctx.regs).pc = resume };
    match cpu {
        ARM9 => exception_handler::handle::<{ ARM9 }>(ctx.asm.emu, comment, ExceptionVector::SoftwareInterrupt),
        ARM7 => exception_handler::handle::<{ ARM7 }>(ctx.asm.emu, comment, ExceptionVector::SoftwareInterrupt),
    }
    if ctx.asm.emu.cpu_is_halted(cpu) {
        // Break out through the store-breakout path; breakout_imm re-derives the resume pc.
        ctx.asm.emu.breakout_imm = true;
        return InstResult::ContinueStore(3);
    }
    let pc = unsafe { (*ctx.regs).pc };
    if pc != resume {
        // The swi redirected execution (soft reset style); tag with the new mode.
        let thumb = ctx.cpsr() & T_BIT != 0;
        return InstResult::Branch(3, (pc & !1) | thumb as u32);
    }
    InstResult::Continue(3)
}

pub(super) fn swi(ctx: &mut Ctx, opcode: u32) -> InstResult {
    swi_common(ctx, (opcode >> 16) as u8, 4)
}

pub(super) fn swi_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    swi_common(ctx, opcode as u8, 2)
}

pub(super) fn mcr(ctx: &mut Ctx, opcode: u32) -> InstResult {
    if ctx.cpu == ARM7 {
        return InstResult::Continue(1);
    }
    let cn = (opcode >> 16) & 0xF;
    let cm = opcode & 0xF;
    let cp = (opcode >> 5) & 0x7;
    let cp15_reg = (cn << 16) | (cm << 8) | cp;
    if cp15_reg == 0x070004 || cp15_reg == 0x070802 {
        // Wait for interrupt, like emit_cp15's halt path.
        unsafe { (*ctx.regs).pc = ctx.inst_addr + 4 };
        unsafe { crate::jit::inst_cpu_regs_handler::cpu_regs_halt() };
        ctx.asm.emu.breakout_imm = true;
        InstResult::ContinueStore(1)
    } else {
        let value = ctx.reg((opcode >> 12) & 0xF);
        ctx.asm.emu.cp15_write(cp15_reg, value);
        InstResult::Continue(1)
    }
}

pub(super) fn mrc(ctx: &mut Ctx, opcode: u32) -> InstResult {
    if ctx.cpu == ARM7 {
        return InstResult::Continue(1);
    }
    let cn = (opcode >> 16) & 0xF;
    let cm = opcode & 0xF;
    let cp = (opcode >> 5) & 0x7;
    let value = ctx.asm.emu.cp15_read((cn << 16) | (cm << 8) | cp);
    let rd = (opcode >> 12) & 0xF;
    if rd != 15 {
        ctx.set_reg(rd, value);
    }
    InstResult::Continue(1)
}

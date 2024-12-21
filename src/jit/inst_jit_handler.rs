use crate::jit::assembler::arm::alu_assembler::AluShiftImm;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::mmap::flush_icache;
use core::slice;
use std::arch::asm;

pub unsafe extern "C" fn inst_slow_mem_patch() {
    let mut lr: u32;
    asm!(
    "mov {}, lr",
    out(reg) lr
    );

    let nop_opcode = AluShiftImm::mov_al(Reg::R0, Reg::R0);

    let mut slow_mem_start = 0;
    for pc_offset in (0..256).step_by(4) {
        let ptr = (lr + pc_offset) as *const u32;
        let opcode = ptr.read();
        if opcode == nop_opcode {
            slow_mem_start = ptr as usize + 4;
            break;
        }
    }
    debug_assert_ne!(slow_mem_start, 0, "{lr:x}");

    let mut slow_mem_end = slow_mem_start;
    let mut fast_mem_end = 0;
    for pc_offset in (4..256).step_by(4) {
        let ptr = (slow_mem_start + pc_offset) as *const u32;
        let opcode = ptr.read();
        let (op, func) = lookup_opcode(opcode);
        if *op == Op::B {
            let inst = func(opcode, *op);
            slow_mem_end = ptr as usize;
            let relative_pc = *inst.operands()[0].as_imm().unwrap() as i32 + 8;
            let target_pc = slow_mem_end as i32 + relative_pc;
            fast_mem_end = target_pc as usize - 4;
            slow_mem_end -= 4;
            break;
        }
    }
    debug_assert_ne!(slow_mem_end, slow_mem_start);
    debug_assert_ne!(fast_mem_end, 0);

    let mut fast_mem_start = 0;
    let mut found_non_op = false;
    for pc_offset in (4..256).step_by(4) {
        let ptr = (fast_mem_end - pc_offset) as *const u32;
        let opcode = ptr.read();
        if found_non_op {
            if opcode == nop_opcode {
                fast_mem_start = ptr as usize;
                break;
            }
        } else if opcode != nop_opcode {
            found_non_op = true;
        }
    }
    debug_assert_ne!(fast_mem_start, 0);

    let slow_mem_size = ((slow_mem_end - slow_mem_start) >> 2) + 1;
    let fast_mem_size = ((fast_mem_end - fast_mem_start) >> 2) + 1;

    let fast_mem = slice::from_raw_parts_mut(fast_mem_start as *mut u32, fast_mem_size);
    let slow_mem = slice::from_raw_parts(slow_mem_start as *const u32, slow_mem_size);
    fast_mem[..slow_mem_size].copy_from_slice(slow_mem);
    fast_mem[slow_mem_size..].fill(nop_opcode);

    flush_icache(fast_mem_start as _, fast_mem_size << 2);
}

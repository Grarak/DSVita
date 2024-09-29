use crate::core::emu::get_regs;
use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
// use crate::jit::assembler::block_asm::BLOCK_LOG;
use crate::jit::inst_info::Operand;
use crate::jit::inst_mem_handler::{inst_mem_handler, inst_mem_handler_multiple, inst_mem_handler_swp};
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::MemoryAmount;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    fn get_inst_mem_handler_func<const THUMB: bool, const WRITE: bool, const MMU: bool>(op: Op, amount: MemoryAmount) -> *const () {
        match amount {
            MemoryAmount::Byte => {
                if op.mem_transfer_single_signed() {
                    inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Byte }, true, MMU> as *const _
                } else {
                    inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Byte }, false, MMU> as *const _
                }
            }
            MemoryAmount::Half => {
                if op.mem_transfer_single_signed() {
                    inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Half }, true, MMU> as *const _
                } else {
                    inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Half }, false, MMU> as *const _
                }
            }
            MemoryAmount::Word => inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Word }, false, MMU> as *const _,
            MemoryAmount::Double => inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Double }, false, MMU> as *const _,
        }
    }

    pub fn emit_single_transfer<const THUMB: bool, const WRITE: bool>(&mut self, block_asm: &mut BlockAsm, pre: bool, write_back: bool, amount: MemoryAmount) {
        let (op, op0, op1, op2) = {
            let inst_info = self.jit_buf.current_inst();
            let operands = inst_info.operands();
            (inst_info.op, *operands[0].as_reg_no_shift().unwrap(), *operands[1].as_reg_no_shift().unwrap(), &operands[2])
        };

        let post_addr_reg = block_asm.new_reg();
        match op2 {
            Operand::Reg { reg, shift } => {
                if op.mem_transfer_single_sub() {
                    block_asm.sub(post_addr_reg, op1, (*reg, *shift))
                } else {
                    block_asm.add(post_addr_reg, op1, (*reg, *shift))
                }
            }
            Operand::Imm(imm) => {
                if op.mem_transfer_single_sub() {
                    block_asm.sub(post_addr_reg, op1, *imm)
                } else {
                    block_asm.add(post_addr_reg, op1, *imm)
                }
            }
            Operand::None => {}
        }

        if op == Op::LdrPcT {
            block_asm.bic(post_addr_reg, post_addr_reg, 0x3);
        }

        let addr_reg = if pre {
            post_addr_reg
        } else {
            let reg = block_asm.new_reg();
            block_asm.mov(reg, op1);
            reg
        };

        if write_back && (WRITE || op0 != op1) {
            block_asm.mov(op1, post_addr_reg);
        }

        block_asm.save_context();

        let op0_addr = get_regs!(self.emu, CPU).get_reg(op0) as *const _ as u32;
        let op0_addr_reg = block_asm.new_reg();
        block_asm.mov(op0_addr_reg, op0_addr);

        if WRITE && !THUMB && op0 == Reg::PC {
            // When op0 is PC, it's read as PC+12
            // Don't need to restore it, since breakouts only happen when PC was written to
            let tmp_pc_reg = block_asm.new_reg();
            block_asm.mov(tmp_pc_reg, self.jit_buf.current_pc + 12);
            block_asm.transfer_write(tmp_pc_reg, op0_addr_reg, 0, false, MemoryAmount::Word);

            block_asm.free_reg(tmp_pc_reg);

            if write_back && op1 == Reg::PC {
                todo!();
            }
        }

        let func_addr = Self::get_inst_mem_handler_func::<THUMB, WRITE, true>(op, amount);
        block_asm.call4(
            func_addr,
            addr_reg,
            op0_addr_reg,
            self.jit_buf.current_pc,
            self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32,
        );

        if !WRITE {
            block_asm.restore_reg(op0);
            if amount == MemoryAmount::Double {
                block_asm.restore_reg(Reg::from(op0 as u8 + 1));
            }
        }
        block_asm.restore_reg(Reg::CPSR);

        block_asm.free_reg(op0_addr_reg);
        block_asm.free_reg(addr_reg);
        block_asm.free_reg(post_addr_reg);
    }

    pub fn emit_single_write(&mut self, block_asm: &mut BlockAsm) {
        let op = self.jit_buf.current_inst().op;
        self.emit_single_transfer::<false, true>(block_asm, op.mem_transfer_pre(), op.mem_transfer_write_back(), MemoryAmount::from(op));
    }

    pub fn emit_single_read(&mut self, block_asm: &mut BlockAsm) {
        let op = self.jit_buf.current_inst().op;
        self.emit_single_transfer::<false, false>(block_asm, op.mem_transfer_pre(), op.mem_transfer_write_back(), MemoryAmount::from(op));
    }

    pub fn emit_multiple_transfer<const THUMB: bool>(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let mut rlist = RegReserve::from(inst_info.opcode & if THUMB { 0xFF } else { 0xFFFF });
        if inst_info.op == Op::PushLrT {
            rlist += Reg::LR;
        } else if inst_info.op == Op::PopPcT {
            rlist += Reg::PC;
        }

        let mut pre = inst_info.op.mem_transfer_pre();
        let decrement = inst_info.op.mem_transfer_decrement();
        if decrement {
            pre = !pre;
        }
        let write_back = inst_info.op.mem_transfer_write_back();
        let has_pc = rlist.is_reserved(Reg::PC);

        let op0 = *inst_info.operands()[0].as_reg_no_shift().unwrap();

        let func_addr: *const () = match (inst_info.op.mem_is_write(), inst_info.op.mem_transfer_user(), pre, write_back, decrement, has_pc) {
            (false, false, false, false, false, false) => inst_mem_handler_multiple::<CPU, THUMB, false, false, false, false, false, false> as _,
            (true, false, false, false, false, false) => inst_mem_handler_multiple::<CPU, THUMB, true, false, false, false, false, false> as _,
            (false, true, false, false, false, false) => inst_mem_handler_multiple::<CPU, THUMB, false, true, false, false, false, false> as _,
            (true, true, false, false, false, false) => inst_mem_handler_multiple::<CPU, THUMB, true, true, false, false, false, false> as _,
            (false, false, true, false, false, false) => inst_mem_handler_multiple::<CPU, THUMB, false, false, true, false, false, false> as _,
            (true, false, true, false, false, false) => inst_mem_handler_multiple::<CPU, THUMB, true, false, true, false, false, false> as _,
            (false, true, true, false, false, false) => inst_mem_handler_multiple::<CPU, THUMB, false, true, true, false, false, false> as _,
            (true, true, true, false, false, false) => inst_mem_handler_multiple::<CPU, THUMB, true, true, true, false, false, false> as _,
            (false, false, false, true, false, false) => inst_mem_handler_multiple::<CPU, THUMB, false, false, false, true, false, false> as _,
            (true, false, false, true, false, false) => inst_mem_handler_multiple::<CPU, THUMB, true, false, false, true, false, false> as _,
            (false, true, false, true, false, false) => inst_mem_handler_multiple::<CPU, THUMB, false, true, false, true, false, false> as _,
            (true, true, false, true, false, false) => inst_mem_handler_multiple::<CPU, THUMB, true, true, false, true, false, false> as _,
            (false, false, true, true, false, false) => inst_mem_handler_multiple::<CPU, THUMB, false, false, true, true, false, false> as _,
            (true, false, true, true, false, false) => inst_mem_handler_multiple::<CPU, THUMB, true, false, true, true, false, false> as _,
            (false, true, true, true, false, false) => inst_mem_handler_multiple::<CPU, THUMB, false, true, true, true, false, false> as _,
            (true, true, true, true, false, false) => inst_mem_handler_multiple::<CPU, THUMB, true, true, true, true, false, false> as _,
            (false, false, false, false, true, false) => inst_mem_handler_multiple::<CPU, THUMB, false, false, false, false, true, false> as _,
            (true, false, false, false, true, false) => inst_mem_handler_multiple::<CPU, THUMB, true, false, false, false, true, false> as _,
            (false, true, false, false, true, false) => inst_mem_handler_multiple::<CPU, THUMB, false, true, false, false, true, false> as _,
            (true, true, false, false, true, false) => inst_mem_handler_multiple::<CPU, THUMB, true, true, false, false, true, false> as _,
            (false, false, true, false, true, false) => inst_mem_handler_multiple::<CPU, THUMB, false, false, true, false, true, false> as _,
            (true, false, true, false, true, false) => inst_mem_handler_multiple::<CPU, THUMB, true, false, true, false, true, false> as _,
            (false, true, true, false, true, false) => inst_mem_handler_multiple::<CPU, THUMB, false, true, true, false, true, false> as _,
            (true, true, true, false, true, false) => inst_mem_handler_multiple::<CPU, THUMB, true, true, true, false, true, false> as _,
            (false, false, false, true, true, false) => inst_mem_handler_multiple::<CPU, THUMB, false, false, false, true, true, false> as _,
            (true, false, false, true, true, false) => inst_mem_handler_multiple::<CPU, THUMB, true, false, false, true, true, false> as _,
            (false, true, false, true, true, false) => inst_mem_handler_multiple::<CPU, THUMB, false, true, false, true, true, false> as _,
            (true, true, false, true, true, false) => inst_mem_handler_multiple::<CPU, THUMB, true, true, false, true, true, false> as _,
            (false, false, true, true, true, false) => inst_mem_handler_multiple::<CPU, THUMB, false, false, true, true, true, false> as _,
            (true, false, true, true, true, false) => inst_mem_handler_multiple::<CPU, THUMB, true, false, true, true, true, false> as _,
            (false, true, true, true, true, false) => inst_mem_handler_multiple::<CPU, THUMB, false, true, true, true, true, false> as _,
            (true, true, true, true, true, false) => inst_mem_handler_multiple::<CPU, THUMB, true, true, true, true, true, false> as _,
            (false, false, false, false, false, true) => inst_mem_handler_multiple::<CPU, THUMB, false, false, false, false, false, true> as _,
            (true, false, false, false, false, true) => inst_mem_handler_multiple::<CPU, THUMB, true, false, false, false, false, true> as _,
            (false, true, false, false, false, true) => inst_mem_handler_multiple::<CPU, THUMB, false, true, false, false, false, true> as _,
            (true, true, false, false, false, true) => inst_mem_handler_multiple::<CPU, THUMB, true, true, false, false, false, true> as _,
            (false, false, true, false, false, true) => inst_mem_handler_multiple::<CPU, THUMB, false, false, true, false, false, true> as _,
            (true, false, true, false, false, true) => inst_mem_handler_multiple::<CPU, THUMB, true, false, true, false, false, true> as _,
            (false, true, true, false, false, true) => inst_mem_handler_multiple::<CPU, THUMB, false, true, true, false, false, true> as _,
            (true, true, true, false, false, true) => inst_mem_handler_multiple::<CPU, THUMB, true, true, true, false, false, true> as _,
            (false, false, false, true, false, true) => inst_mem_handler_multiple::<CPU, THUMB, false, false, false, true, false, true> as _,
            (true, false, false, true, false, true) => inst_mem_handler_multiple::<CPU, THUMB, true, false, false, true, false, true> as _,
            (false, true, false, true, false, true) => inst_mem_handler_multiple::<CPU, THUMB, false, true, false, true, false, true> as _,
            (true, true, false, true, false, true) => inst_mem_handler_multiple::<CPU, THUMB, true, true, false, true, false, true> as _,
            (false, false, true, true, false, true) => inst_mem_handler_multiple::<CPU, THUMB, false, false, true, true, false, true> as _,
            (true, false, true, true, false, true) => inst_mem_handler_multiple::<CPU, THUMB, true, false, true, true, false, true> as _,
            (false, true, true, true, false, true) => inst_mem_handler_multiple::<CPU, THUMB, false, true, true, true, false, true> as _,
            (true, true, true, true, false, true) => inst_mem_handler_multiple::<CPU, THUMB, true, true, true, true, false, true> as _,
            (false, false, false, false, true, true) => inst_mem_handler_multiple::<CPU, THUMB, false, false, false, false, true, true> as _,
            (true, false, false, false, true, true) => inst_mem_handler_multiple::<CPU, THUMB, true, false, false, false, true, true> as _,
            (false, true, false, false, true, true) => inst_mem_handler_multiple::<CPU, THUMB, false, true, false, false, true, true> as _,
            (true, true, false, false, true, true) => inst_mem_handler_multiple::<CPU, THUMB, true, true, false, false, true, true> as _,
            (false, false, true, false, true, true) => inst_mem_handler_multiple::<CPU, THUMB, false, false, true, false, true, true> as _,
            (true, false, true, false, true, true) => inst_mem_handler_multiple::<CPU, THUMB, true, false, true, false, true, true> as _,
            (false, true, true, false, true, true) => inst_mem_handler_multiple::<CPU, THUMB, false, true, true, false, true, true> as _,
            (true, true, true, false, true, true) => inst_mem_handler_multiple::<CPU, THUMB, true, true, true, false, true, true> as _,
            (false, false, false, true, true, true) => inst_mem_handler_multiple::<CPU, THUMB, false, false, false, true, true, true> as _,
            (true, false, false, true, true, true) => inst_mem_handler_multiple::<CPU, THUMB, true, false, false, true, true, true> as _,
            (false, true, false, true, true, true) => inst_mem_handler_multiple::<CPU, THUMB, false, true, false, true, true, true> as _,
            (true, true, false, true, true, true) => inst_mem_handler_multiple::<CPU, THUMB, true, true, false, true, true, true> as _,
            (false, false, true, true, true, true) => inst_mem_handler_multiple::<CPU, THUMB, false, false, true, true, true, true> as _,
            (true, false, true, true, true, true) => inst_mem_handler_multiple::<CPU, THUMB, true, false, true, true, true, true> as _,
            (false, true, true, true, true, true) => inst_mem_handler_multiple::<CPU, THUMB, false, true, true, true, true, true> as _,
            (true, true, true, true, true, true) => inst_mem_handler_multiple::<CPU, THUMB, true, true, true, true, true, true> as _,
        };

        block_asm.save_context();
        block_asm.call3(
            func_addr,
            rlist.0 | ((op0 as u32) << 16),
            self.jit_buf.current_pc,
            self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32,
        );

        let mut restore_regs = RegReserve::new();
        if write_back {
            restore_regs += op0;
        }
        if !inst_info.op.mem_is_write() {
            restore_regs += rlist;
        }
        for reg in restore_regs {
            block_asm.restore_reg(reg);
        }

        block_asm.restore_reg(Reg::CPSR);
    }

    pub fn emit_swp(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        let operands = inst_info.operands();
        let op0 = *operands[0].as_reg_no_shift().unwrap();
        let op1 = *operands[1].as_reg_no_shift().unwrap();
        let op2 = *operands[2].as_reg_no_shift().unwrap();

        let op0_total_cycles = (op0 as u32) << 16 | (self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32);

        let func_addr = if inst_info.op == Op::Swpb {
            inst_mem_handler_swp::<CPU, { MemoryAmount::Byte }> as *const ()
        } else {
            inst_mem_handler_swp::<CPU, { MemoryAmount::Word }> as *const ()
        };

        block_asm.save_context();
        block_asm.call4(func_addr, op1, op2, self.jit_buf.current_pc, op0_total_cycles);
        block_asm.restore_reg(op0);
        block_asm.restore_reg(Reg::CPSR);
    }
}

use crate::core::emu::{get_mmu, get_regs};
use crate::core::memory::mmu;
use crate::core::CpuType;
use crate::jit::assembler::BlockOperand;
use crate::jit::inst_info::Operand;
use crate::jit::inst_mem_handler::{inst_mem_handler, inst_mem_handler_multiple, inst_mem_handler_swp};
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, Op, ShiftType};

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

    pub fn emit_single_transfer<const THUMB: bool, const WRITE: bool>(&mut self, buf_index: usize, pc: u32, pre: bool, write_back: bool, amount: MemoryAmount) {
        if !WRITE && amount != MemoryAmount::Double {
            let func = match (write_back, pre) {
                (false, false) => Self::emit_single_read_transfer::<THUMB, false, false>,
                (true, false) => Self::emit_single_read_transfer::<THUMB, true, false>,
                (false, true) => Self::emit_single_read_transfer::<THUMB, false, true>,
                (true, true) => Self::emit_single_read_transfer::<THUMB, true, true>,
            };
            func(self, buf_index, pc, amount);
            return;
        }

        let jit_asm_addr = self as *mut _ as u32;

        let (op, op0, op1, op2) = {
            let inst_info = &self.jit_buf.insts[buf_index];
            let operands = inst_info.operands();
            (inst_info.op, *operands[0].as_reg_no_shift().unwrap(), *operands[1].as_reg_no_shift().unwrap(), &operands[2])
        };

        self.jit_buf.emit_opcodes.extend(if THUMB { &self.restore_host_thumb_opcodes } else { &self.restore_host_opcodes });

        let mut block_asm = self.block_asm_buf.new_asm(get_regs!(self.emu, CPU));

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

        let addr_reg = block_asm.new_reg();

        if write_back && (WRITE || op0 != op1) {
            block_asm.mov(addr_reg, if pre { post_addr_reg } else { op1.into() });
            block_asm.mov(op1, post_addr_reg);
        } else {
            block_asm.mov(addr_reg, if pre { post_addr_reg } else { op1.into() });
        }

        block_asm.save_context();

        let op0_addr = get_regs!(self.emu, CPU).get_reg(op0) as *const _ as u32;
        let func_addr = Self::get_inst_mem_handler_func::<THUMB, WRITE, true>(op, amount);
        block_asm.call4(func_addr, addr_reg, op0_addr, pc, jit_asm_addr);

        let opcodes = block_asm.finalize(pc + if THUMB { 4 } else { 8 });
        self.jit_buf.emit_opcodes.extend(opcodes);

        self.jit_buf.emit_opcodes.extend(if THUMB { &self.restore_guest_thumb_opcodes } else { &self.restore_guest_opcodes });
    }

    pub fn emit_single_read_transfer<const THUMB: bool, const WRITE_BACK: bool, const PRE: bool>(&mut self, buf_index: usize, pc: u32, amount: MemoryAmount) {
        let jit_asm_addr = self as *mut _ as u32;

        let (op, op0, op1, op2) = {
            let inst_info = &self.jit_buf.insts[buf_index];
            let operands = inst_info.operands();
            (inst_info.op, *operands[0].as_reg_no_shift().unwrap(), *operands[1].as_reg_no_shift().unwrap(), &operands[2])
        };

        self.jit_buf.emit_opcodes.extend(if THUMB { &self.restore_host_thumb_opcodes } else { &self.restore_host_opcodes });

        let mut block_asm = self.block_asm_buf.new_asm(get_regs!(self.emu, CPU));

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

        let addr_reg = if PRE { post_addr_reg } else { op1.into() };

        let mmu_ptr_reg = block_asm.new_reg();
        let mmu_index_reg = block_asm.new_reg();
        let mmu_page_addr = block_asm.new_reg();
        let backup_cpsr = block_asm.new_reg();

        block_asm.mov(mmu_ptr_reg, get_mmu!(self.emu, CPU).get_mmu_ptr() as u32);
        block_asm.mov(mmu_index_reg, (BlockOperand::from(addr_reg), ShiftType::Lsr, mmu::MMU_BLOCK_SHIFT.into()));
        block_asm.transfer_read(mmu_page_addr, mmu_ptr_reg, (BlockOperand::from(mmu_index_reg), ShiftType::Lsl, 2.into()), false, MemoryAmount::Word);
        block_asm.mrs_cpsr(backup_cpsr);
        block_asm.cmp(mmu_page_addr, 0);

        let slow_read_label = block_asm.new_label();
        let slow_read_end_label = block_asm.new_label();

        block_asm.branch(slow_read_label, Cond::EQ);

        {
            let physical_addr = block_asm.new_reg();
            block_asm.msr_cpsr(backup_cpsr);
            block_asm.mov(physical_addr, addr_reg);
            block_asm.bfc(physical_addr, 12, 20);
            if amount == MemoryAmount::Word {
                block_asm.bic(physical_addr, physical_addr, 0x3);
            }
            let read_value_reg = block_asm.new_reg();
            block_asm.transfer_read(read_value_reg, mmu_page_addr, physical_addr, op.mem_transfer_single_signed(), amount);
            if amount == MemoryAmount::Word {
                block_asm.mov(physical_addr, (BlockOperand::from(addr_reg), ShiftType::Lsl, 3.into()));
                block_asm.mov(op0, (read_value_reg, ShiftType::Ror, physical_addr));
            } else {
                block_asm.mov(op0, read_value_reg);
            }

            if WRITE_BACK && op0 != op1 {
                block_asm.mov(op1, post_addr_reg);
            }
            block_asm.save_context();
            block_asm.branch(slow_read_end_label, Cond::AL);
        }

        block_asm.label(slow_read_label);

        {
            let func_addr = Self::get_inst_mem_handler_func::<THUMB, false, false>(op, amount);

            if WRITE_BACK && op0 != op1 {
                block_asm.mov(op1, post_addr_reg);
            }

            block_asm.mov(Reg::CPSR, backup_cpsr);
            block_asm.save_context();

            block_asm.call4(func_addr, addr_reg, get_regs!(self.emu, CPU).get_reg(op0) as *const _ as u32, pc, jit_asm_addr);
        }

        block_asm.label(slow_read_end_label);

        let opcodes = block_asm.finalize(pc + if THUMB { 4 } else { 8 });
        self.jit_buf.emit_opcodes.extend(opcodes);

        self.jit_buf.emit_opcodes.extend(if THUMB { &self.restore_guest_thumb_opcodes } else { &self.restore_guest_opcodes });
    }

    pub fn emit_multiple_transfer<const THUMB: bool>(&mut self, buf_index: usize, pc: u32) {
        let jit_asm_addr = self as *mut _ as _;
        let inst_info = &self.jit_buf.insts[buf_index];

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

        let func_addr = match (inst_info.op.mem_is_write(), inst_info.op.mem_transfer_user(), pre, write_back, decrement, has_pc) {
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

        self.jit_buf.emit_opcodes.extend(self.emit_call_host_func(
            |_, _| {},
            &[
                Some(pc),
                Some(rlist.0 | ((op0 as u32) << 16)),
                Some(self.jit_buf.insts_cycle_counts[buf_index] as u32),
                Some(jit_asm_addr),
            ],
            func_addr,
        ));
    }

    pub fn emit_str(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.insts[buf_index].op;
        self.emit_single_transfer::<false, true>(buf_index, pc, op.mem_transfer_pre(), op.mem_transfer_write_back(), MemoryAmount::from(op));
    }

    pub fn emit_ldr(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.insts[buf_index].op;
        self.emit_single_transfer::<false, false>(buf_index, pc, op.mem_transfer_pre(), op.mem_transfer_write_back(), MemoryAmount::from(op));
    }

    pub fn emit_swp(&mut self, buf_index: usize, pc: u32) {
        let jit_asm_addr = self as *mut _ as _;
        let inst_info = &self.jit_buf.insts[buf_index];
        let operands = inst_info.operands();
        let op0 = *operands[0].as_reg_no_shift().unwrap();
        let op1 = *operands[1].as_reg_no_shift().unwrap();
        let op2 = *operands[2].as_reg_no_shift().unwrap();

        let reg_arg = ((op2 as u32) << 16) | ((op1 as u32) << 8) | (op0 as u32);

        let func_addr = if inst_info.op == Op::Swpb {
            inst_mem_handler_swp::<CPU, { MemoryAmount::Byte }> as *const ()
        } else {
            inst_mem_handler_swp::<CPU, { MemoryAmount::Word }> as *const ()
        };

        self.jit_buf
            .emit_opcodes
            .extend(self.emit_call_host_func(|_, _| {}, &[Some(reg_arg), Some(pc), Some(jit_asm_addr)], func_addr));
    }
}

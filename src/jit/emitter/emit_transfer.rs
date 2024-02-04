use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::inst_info::{Operand, Shift, ShiftValue};
use crate::jit::inst_mem_handler::{
    inst_mem_handler, inst_mem_handler_multiple, inst_mem_handler_multiple_user,
    inst_mem_handler_swp, InstMemMultipleArgs,
};
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, Op, ShiftType};
use bilge::prelude::*;
use std::ptr;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_single_transfer<const THUMB: bool, const WRITE: bool>(
        &mut self,
        buf_index: usize,
        pc: u32,
        pre: bool,
        write_back: bool,
        amount: MemoryAmount,
    ) {
        let after_host_restore = |asm: &mut Self| {
            let inst_info = &asm.jit_buf.instructions[buf_index];
            let opcodes = &mut asm.jit_buf.emit_opcodes;

            let operands = inst_info.operands();
            let op0 = *operands[0].as_reg_no_shift().unwrap();
            let og_op1 = operands[1].as_reg_no_shift().unwrap();

            let mut reg_reserve = (!reg_reserve!(*og_op1)).get_gp_regs();

            let handle_emulated =
                |reg: Reg, reg_reserve: &mut RegReserve, opcodes: &mut Vec<u32>| {
                    if reg.is_emulated() || reg == Reg::SP {
                        let tmp_reg = reg_reserve.pop().unwrap();
                        if reg == Reg::PC {
                            opcodes.extend(AluImm::mov32(tmp_reg, pc + if THUMB { 4 } else { 8 }));
                        } else {
                            opcodes.extend(asm.thread_regs.borrow().emit_get_reg(tmp_reg, reg));
                        }
                        tmp_reg
                    } else {
                        reg
                    }
                };

            let add_to_op1 = match &operands[2] {
                Operand::Reg { reg, shift } => {
                    let reg = handle_emulated(*reg, &mut reg_reserve, opcodes);
                    Some(match shift {
                        None => {
                            reg_reserve -= reg;
                            reg
                        }
                        Some(shift) => {
                            let tmp_reg = reg_reserve.pop().unwrap();
                            let mut handle_shift =
                                |shift_type: ShiftType, value: ShiftValue| match value {
                                    ShiftValue::Reg(shift_reg) => {
                                        let shift_reg =
                                            handle_emulated(shift_reg, &mut reg_reserve, opcodes);
                                        AluReg::mov(tmp_reg, reg, shift_type, shift_reg, Cond::AL)
                                    }
                                    ShiftValue::Imm(imm) => {
                                        AluShiftImm::mov(tmp_reg, reg, shift_type, imm, Cond::AL)
                                    }
                                };
                            let opcode = match shift {
                                Shift::Lsl(v) => handle_shift(ShiftType::Lsl, *v),
                                Shift::Lsr(v) => handle_shift(ShiftType::Lsr, *v),
                                Shift::Asr(v) => handle_shift(ShiftType::Asr, *v),
                                Shift::Ror(v) => handle_shift(ShiftType::Ror, *v),
                            };
                            opcodes.push(opcode);
                            tmp_reg
                        }
                    })
                }
                Operand::Imm(imm) => {
                    let tmp_reg = reg_reserve.pop().unwrap();
                    opcodes.extend(AluImm::mov32(tmp_reg, *imm));
                    Some(tmp_reg)
                }
                _ => None,
            };

            let op1 = handle_emulated(*og_op1, &mut reg_reserve, opcodes);
            let addr_reg = reg_reserve.pop_call_reserved().unwrap();
            if let Some(reg) = add_to_op1 {
                opcodes.push(AluShiftImm::add_al(addr_reg, op1, reg));
            }

            if inst_info.op == Op::LdrPcT {
                opcodes.push(AluImm::bic_al(addr_reg, addr_reg, 3));
            }

            if pre {
                opcodes.push(AluShiftImm::mov_al(Reg::R0, addr_reg));
            } else if op1 != Reg::R0 {
                opcodes.push(AluShiftImm::mov_al(Reg::R0, op1));
            }
            opcodes.extend(AluImm::mov32(
                Reg::R1,
                asm.thread_regs.borrow_mut().get_reg_value_mut(op0) as *mut _ as _,
            ));
            if WRITE && op0 == Reg::PC {
                opcodes.extend(AluImm::mov32(Reg::R3, pc + if THUMB { 4 } else { 8 }));
                opcodes.push(LdrStrImm::str_al(Reg::R3, Reg::R1));
            }

            if write_back {
                Some((*og_op1, addr_reg))
            } else {
                None
            }
        };

        let before_guest_restore = |asm: &mut Self, write_back_regs: Option<(Reg, Reg)>| {
            if let Some((op1, write_back)) = write_back_regs {
                asm.jit_buf
                    .emit_opcodes
                    .extend(
                        asm.thread_regs
                            .borrow()
                            .emit_set_reg(op1, write_back, Reg::R0),
                    );
            }
        };

        let mem_handler_addr = ptr::addr_of_mut!(self.inst_mem_handler) as u32;
        let func_addr = match amount {
            MemoryAmount::Byte => {
                inst_mem_handler::<CPU, WRITE, { MemoryAmount::Byte }> as *const _
            }
            MemoryAmount::Half => {
                inst_mem_handler::<CPU, WRITE, { MemoryAmount::Half }> as *const _
            }
            MemoryAmount::Word => {
                inst_mem_handler::<CPU, WRITE, { MemoryAmount::Word }> as *const _
            }
            MemoryAmount::Double => {
                inst_mem_handler::<CPU, WRITE, { MemoryAmount::Double }> as *const _
            }
        };

        self.emit_call_host_func(
            after_host_restore,
            before_guest_restore,
            &[None, None, Some(mem_handler_addr)],
            func_addr,
        );
    }

    pub fn emit_multiple_transfer<const THUMB: bool>(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];
        let mem_handler_addr = ptr::addr_of!(self.inst_mem_handler) as u32;

        let mut rlist = (inst_info.opcode & if THUMB { 0xFF } else { 0xFFFF }) as u16;
        if inst_info.op == Op::PushLrT {
            rlist |= 1 << Reg::LR as u8;
        } else if inst_info.op == Op::PopPcT {
            rlist |= 1 << Reg::PC as u8;
        }

        let mut pre = inst_info.op.mem_transfer_pre();
        let decrement = inst_info.op.mem_transfer_decrement();
        if decrement {
            pre = !pre;
        }
        let write_back = inst_info.op.mem_transfer_write_back();

        let op0 = *inst_info.operands()[0].as_reg_no_shift().unwrap();

        let args = u32::from(InstMemMultipleArgs::new(
            u1::from(pre),
            u1::from(write_back),
            u1::from(decrement),
            u5::new(op0 as u8),
            u24::from(rlist),
        ));

        let func_addr = match (
            inst_info.op.mem_transfer_user(),
            inst_info.op.mem_is_write(),
        ) {
            (true, true) => inst_mem_handler_multiple_user::<CPU, true> as _,
            (true, false) => inst_mem_handler_multiple_user::<CPU, false> as _,
            (false, true) => inst_mem_handler_multiple::<CPU, THUMB, true> as _,
            (false, false) => inst_mem_handler_multiple::<CPU, THUMB, false> as _,
        };

        self.emit_call_host_func(
            |_| {},
            |_, _| {},
            &[Some(mem_handler_addr), Some(pc), Some(args)],
            func_addr,
        );
    }

    pub fn emit_str(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        self.emit_single_transfer::<false, true>(
            buf_index,
            pc,
            op.mem_transfer_pre(),
            op.mem_transfer_write_back(),
            MemoryAmount::from(op),
        );
    }

    pub fn emit_ldr(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        self.emit_single_transfer::<false, false>(
            buf_index,
            pc,
            op.mem_transfer_pre(),
            op.mem_transfer_write_back(),
            MemoryAmount::from(op),
        );
    }

    pub fn emit_swp(&mut self, buf_index: usize, _: u32) {
        let op = self.jit_buf.instructions[buf_index].op;

        let mem_handler_addr = ptr::addr_of!(self.inst_mem_handler) as u32;
        let func_addr = if op == Op::Swpb {
            inst_mem_handler_swp::<CPU, { MemoryAmount::Byte }> as *const ()
        } else {
            inst_mem_handler_swp::<CPU, { MemoryAmount::Word }> as *const ()
        };

        self.emit_call_host_func(
            |asm| {
                let inst_info = &asm.jit_buf.instructions[buf_index];

                let operands = inst_info.operands();
                let op0 = *operands[0].as_reg_no_shift().unwrap();
                let op1 = *operands[1].as_reg_no_shift().unwrap();
                let op2 = *operands[2].as_reg_no_shift().unwrap();

                let mut reg_reserve =
                    (!reg_reserve!(Reg::R0, Reg::R1, Reg::R2, Reg::R3, op1, op2)).get_gp_regs();

                let mut handle_emulated = |reg: Reg| {
                    if reg == Reg::LR || reg == Reg::SP {
                        let new_reg = reg_reserve.pop().unwrap();
                        asm.jit_buf
                            .emit_opcodes
                            .extend(asm.thread_regs.borrow().emit_get_reg(new_reg, reg));
                        new_reg
                    } else {
                        reg
                    }
                };

                let op1 = handle_emulated(op1);
                let mut op2 = handle_emulated(op2);

                if op2 == Reg::R2 {
                    let new_op2 = reg_reserve.pop().unwrap();
                    asm.jit_buf
                        .emit_opcodes
                        .push(AluShiftImm::mov_al(new_op2, op2));
                    op2 = new_op2;
                }

                if op1 != Reg::R2 {
                    asm.jit_buf
                        .emit_opcodes
                        .push(AluShiftImm::mov_al(Reg::R2, op1));
                }
                if op2 != Reg::R3 {
                    asm.jit_buf
                        .emit_opcodes
                        .push(AluShiftImm::mov_al(Reg::R3, op2));
                }
                asm.jit_buf.emit_opcodes.extend(AluImm::mov32(
                    Reg::R1,
                    asm.thread_regs.borrow_mut().get_reg_value_mut(op0) as *mut _ as _,
                ));
            },
            |_, _| {},
            &[Some(mem_handler_addr), None, None, None],
            func_addr,
        );
    }
}

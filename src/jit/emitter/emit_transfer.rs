use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::inst_info::{Operand, Shift, ShiftValue};
use crate::jit::inst_mem_handler::{inst_mem_handler, inst_mem_handler_multiple};
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, Op, ShiftType};
use std::ptr;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_transfer_indirect(
        &mut self,
        func_addr: *const (),
        opcode: u32,
        pc: u32,
        flags: u8,
    ) {
        let mem_handler_addr = ptr::addr_of_mut!(self.inst_mem_handler) as u32;

        self.emit_call_host_func(
            |_| {},
            |_, _| {},
            &[
                Some(mem_handler_addr),
                Some(opcode),
                Some(pc),
                Some(flags as u32),
            ],
            func_addr,
        );
    }

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
                                Shift::LSL(v) => handle_shift(ShiftType::LSL, *v),
                                Shift::LSR(v) => handle_shift(ShiftType::LSR, *v),
                                Shift::ASR(v) => handle_shift(ShiftType::ASR, *v),
                                Shift::ROR(v) => handle_shift(ShiftType::ROR, *v),
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
            MemoryAmount::BYTE => {
                inst_mem_handler::<CPU, WRITE, { MemoryAmount::BYTE }> as *const _
            }
            MemoryAmount::HALF => {
                inst_mem_handler::<CPU, WRITE, { MemoryAmount::HALF }> as *const _
            }
            MemoryAmount::WORD => {
                inst_mem_handler::<CPU, WRITE, { MemoryAmount::WORD }> as *const _
            }
            MemoryAmount::DOUBLE => {
                inst_mem_handler::<CPU, WRITE, { MemoryAmount::DOUBLE }> as *const _
            }
        };

        self.emit_call_host_func(
            after_host_restore,
            before_guest_restore,
            &[None, None, Some(mem_handler_addr)],
            func_addr,
        );
    }

    pub fn emit_str(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;

        let pre = match op {
            Op::StrOfim | Op::StrOfip | Op::StrbOfip | Op::StrhOfip | Op::StrPrim => true,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::StrOfim | Op::StrOfip | Op::StrbOfip | Op::StrhOfip => false,
            Op::StrPrim => true,
            _ => todo!("{:?}", op),
        };

        self.emit_single_transfer::<false, true>(
            buf_index,
            pc,
            pre,
            write_back,
            MemoryAmount::from(op),
        );
    }

    pub fn emit_ldr(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let pre = match op {
            Op::LdrbOfrpll
            | Op::LdrhOfip
            | Op::LdrOfip
            | Op::LdrOfim
            | Op::LdrbOfrplr
            | Op::LdrOfrpll => true,
            Op::LdrPtip => false,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::LdrbOfrpll
            | Op::LdrhOfip
            | Op::LdrOfip
            | Op::LdrOfim
            | Op::LdrbOfrplr
            | Op::LdrOfrpll => false,
            Op::LdrPtip => true,
            _ => todo!("{:?}", op),
        };

        self.emit_single_transfer::<false, false>(
            buf_index,
            pc,
            pre,
            write_back,
            MemoryAmount::from(op),
        );
    }

    pub fn emit_stm(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let mut pre = match op {
            Op::Stmia | Op::StmiaW => false,
            Op::Stmdb | Op::StmdbW => true,
            _ => todo!("{:?}", op),
        };

        let decrement = match op {
            Op::Stmia | Op::StmiaW => false,
            Op::Stmdb | Op::StmdbW => {
                pre = !pre;
                true
            }
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::Stmia | Op::Stmdb => false,
            Op::StmiaW | Op::StmdbW => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((decrement as u8) << 2);
        self.emit_transfer_indirect(
            inst_mem_handler_multiple::<CPU, false, true> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }

    pub fn emit_ldm(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        let pre = match op {
            Op::Ldmia | Op::LdmiaW => false,
            _ => todo!("{:?}", op),
        };

        let decrement = match op {
            Op::Ldmia | Op::LdmiaW => false,
            _ => todo!("{:?}", op),
        };

        let write_back = match op {
            Op::Ldmia => false,
            Op::LdmiaW => true,
            _ => todo!("{:?}", op),
        };

        let flags = (pre as u8) | ((write_back as u8) << 1) | ((decrement as u8) << 2);
        self.emit_transfer_indirect(
            inst_mem_handler_multiple::<CPU, false, false> as _,
            self.jit_buf.instructions[buf_index].opcode,
            pc,
            flags,
        );
    }
}

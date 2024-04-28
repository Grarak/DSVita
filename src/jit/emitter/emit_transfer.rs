use crate::emu::emu::{get_mmu, get_regs};
use crate::emu::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg, AluShiftImm, Bfc};
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::arm::transfer_assembler::{
    LdrStrImm, LdrStrReg, LdrStrRegSBHD, Mrs, Msr,
};
use crate::jit::emitter::emit::RegPushPopHandler;
use crate::jit::inst_info::{Operand, Shift, ShiftValue};
use crate::jit::inst_mem_handler::{
    inst_mem_handler, inst_mem_handler_multiple, inst_mem_handler_swp,
};
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, Op, ShiftType};
use std::ptr;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    fn get_inst_mem_handler_func<const THUMB: bool, const WRITE: bool, const MMU: bool>(
        op: Op,
        amount: MemoryAmount,
    ) -> *const () {
        match amount {
            MemoryAmount::Byte => {
                if op.mem_transfer_single_signed() {
                    inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Byte }, true, MMU>
                        as *const _
                } else {
                    inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Byte }, false, MMU>
                        as *const _
                }
            }
            MemoryAmount::Half => {
                if op.mem_transfer_single_signed() {
                    inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Half }, true, MMU>
                        as *const _
                } else {
                    inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Half }, false, MMU>
                        as *const _
                }
            }
            MemoryAmount::Word => {
                inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Word }, false, MMU>
                    as *const _
            }
            MemoryAmount::Double => {
                inst_mem_handler::<CPU, THUMB, WRITE, { MemoryAmount::Double }, false, MMU>
                    as *const _
            }
        }
    }

    pub fn emit_single_transfer<const THUMB: bool, const WRITE: bool>(
        &mut self,
        buf_index: usize,
        pc: u32,
        pre: bool,
        write_back: bool,
        amount: MemoryAmount,
    ) {
        // if !WRITE && amount != MemoryAmount::Double {
        //     let func = match (write_back, pre) {
        //         (false, false) => Self::emit_single_read_transfer::<THUMB, false, false>,
        //         (true, false) => Self::emit_single_read_transfer::<THUMB, true, false>,
        //         (false, true) => Self::emit_single_read_transfer::<THUMB, false, true>,
        //         (true, true) => Self::emit_single_read_transfer::<THUMB, true, true>,
        //     };
        //     func(self, buf_index, pc, amount);
        //     return;
        // }

        let jit_asm_addr = self as *mut _ as _;

        let after_host_restore = |asm: &Self, opcodes: &mut Vec<u32>| {
            let inst_info = &asm.jit_buf.instructions[buf_index];

            let operands = inst_info.operands();
            let op0 = *operands[0].as_reg_no_shift().unwrap();
            let og_op1 = operands[1].as_reg_no_shift().unwrap();
            let op2 = &operands[2];

            let mut reg_reserve = RegReserve::gp() - inst_info.src_regs - inst_info.out_regs;

            let handle_emulated =
                |reg: Reg, reg_reserve: &mut RegReserve, opcodes: &mut Vec<u32>| {
                    if reg.is_emulated() || reg == Reg::SP {
                        let tmp_reg = reg_reserve.pop().unwrap();
                        if reg == Reg::PC {
                            opcodes.extend(AluImm::mov32(tmp_reg, pc + if THUMB { 4 } else { 8 }));
                        } else {
                            opcodes.extend(get_regs!(asm.emu, CPU).emit_get_reg(tmp_reg, reg));
                        }
                        tmp_reg
                    } else {
                        reg
                    }
                };

            let op1 = handle_emulated(*og_op1, &mut reg_reserve, opcodes);
            let (op2, op2_shift) = match op2 {
                Operand::Reg { reg, shift } => {
                    let reg = handle_emulated(*reg, &mut reg_reserve, opcodes);
                    match shift {
                        None => (Some(reg), None),
                        Some(shift) => {
                            let mut handle_shift =
                                |shift_type: ShiftType, value: ShiftValue| match value {
                                    ShiftValue::Reg(shift_reg) => {
                                        let reg =
                                            handle_emulated(shift_reg, &mut reg_reserve, opcodes);
                                        (Operand::reg(reg), shift_type)
                                    }
                                    ShiftValue::Imm(imm) => (Operand::imm(imm as u32), shift_type),
                                };
                            (
                                Some(reg),
                                Some(match shift {
                                    Shift::Lsl(v) => handle_shift(ShiftType::Lsl, *v),
                                    Shift::Lsr(v) => handle_shift(ShiftType::Lsr, *v),
                                    Shift::Asr(v) => handle_shift(ShiftType::Asr, *v),
                                    Shift::Ror(v) => handle_shift(ShiftType::Ror, *v),
                                }),
                            )
                        }
                    }
                }
                Operand::Imm(imm) => {
                    let tmp_reg = reg_reserve.pop().unwrap();
                    opcodes.extend(AluImm::mov32(tmp_reg, *imm));
                    (Some(tmp_reg), None)
                }
                _ => (None, None),
            };

            let addr_reg = reg_reserve.pop_call_reserved().unwrap();

            if let Some(op2) = op2 {
                match op2_shift {
                    Some((reg, shift_type)) => match reg {
                        Operand::Reg { reg, .. } => {
                            opcodes.push(if inst_info.op.mem_transfer_single_sub() {
                                AluReg::sub(addr_reg, op1, op2, shift_type, reg, Cond::AL)
                            } else {
                                AluReg::add(addr_reg, op1, op2, shift_type, reg, Cond::AL)
                            });
                        }
                        Operand::Imm(imm) => {
                            opcodes.push(if inst_info.op.mem_transfer_single_sub() {
                                AluShiftImm::sub(
                                    addr_reg,
                                    op1,
                                    op2,
                                    shift_type,
                                    imm as u8,
                                    Cond::AL,
                                )
                            } else {
                                AluShiftImm::add(
                                    addr_reg,
                                    op1,
                                    op2,
                                    shift_type,
                                    imm as u8,
                                    Cond::AL,
                                )
                            });
                        }
                        Operand::None => {
                            unreachable!()
                        }
                    },
                    None => {
                        opcodes.push(if inst_info.op.mem_transfer_single_sub() {
                            AluShiftImm::sub_al(addr_reg, op1, op2)
                        } else {
                            AluShiftImm::add_al(addr_reg, op1, op2)
                        });
                    }
                }
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
                get_regs!(asm.emu, CPU).get_reg(op0) as *const _ as _,
            ));
            if WRITE && op0 == Reg::PC {
                opcodes.extend(AluImm::mov32(Reg::R3, pc + if THUMB { 4 } else { 12 }));
                opcodes.push(LdrStrImm::str_al(Reg::R3, Reg::R1));
            }

            if write_back && (WRITE || op0 != *og_op1) {
                opcodes.extend(get_regs!(asm.emu, CPU).emit_set_reg(*og_op1, addr_reg, Reg::R2));
            }
        };

        let func_addr = Self::get_inst_mem_handler_func::<THUMB, WRITE, true>(
            self.jit_buf.instructions[buf_index].op,
            amount,
        );

        self.jit_buf.emit_opcodes.extend(self.emit_call_host_func(
            after_host_restore,
            &[None, None, Some(pc), Some(jit_asm_addr)],
            func_addr,
        ));
    }

    pub fn emit_single_read_transfer<const THUMB: bool, const WRITE_BACK: bool, const PRE: bool>(
        &mut self,
        buf_index: usize,
        pc: u32,
        amount: MemoryAmount,
    ) {
        let jit_asm_addr = self as *mut _ as _;
        let inst_info = &self.jit_buf.instructions[buf_index];
        let mut opcodes = Vec::new();

        let operands = inst_info.operands();
        let op0 = *operands[0].as_reg_no_shift().unwrap();
        let og_op1 = operands[1].as_reg_no_shift().unwrap();
        let op2 = &operands[2];

        let mut reg_reserve = RegPushPopHandler::new();
        reg_reserve.set_regs_to_skip(inst_info.src_regs + inst_info.out_regs);

        let mut reusable_regs = RegReserve::new();

        let mut handle_emulated = |reg: Reg, reusable: bool| {
            if reg.is_emulated() {
                let tmp_reg = reg_reserve.pop().unwrap();
                if reg == Reg::PC {
                    opcodes.extend(AluImm::mov32(tmp_reg, pc + if THUMB { 4 } else { 8 }));
                } else {
                    opcodes.extend(get_regs!(self.emu, CPU).emit_get_reg(tmp_reg, reg));
                }
                if reusable {
                    reusable_regs += tmp_reg;
                }
                tmp_reg
            } else {
                reg
            }
        };

        let op1 = handle_emulated(*og_op1, false);
        let (op2, op2_shift) = match op2 {
            Operand::Reg { reg, shift } => {
                let reg = handle_emulated(*reg, true);
                match shift {
                    None => (Some(reg), None),
                    Some(shift) => {
                        let mut handle_shift =
                            |shift_type: ShiftType, value: ShiftValue| match value {
                                ShiftValue::Reg(shift_reg) => {
                                    let reg = handle_emulated(shift_reg, true);
                                    (Operand::reg(reg), shift_type)
                                }
                                ShiftValue::Imm(imm) => (Operand::imm(imm as u32), shift_type),
                            };
                        (
                            Some(reg),
                            Some(match shift {
                                Shift::Lsl(v) => handle_shift(ShiftType::Lsl, *v),
                                Shift::Lsr(v) => handle_shift(ShiftType::Lsr, *v),
                                Shift::Asr(v) => handle_shift(ShiftType::Asr, *v),
                                Shift::Ror(v) => handle_shift(ShiftType::Ror, *v),
                            }),
                        )
                    }
                }
            }
            Operand::Imm(imm) => {
                let tmp_reg = reg_reserve.pop().unwrap();
                opcodes.extend(AluImm::mov32(tmp_reg, *imm));
                reusable_regs += tmp_reg;
                (Some(tmp_reg), None)
            }
            _ => (None, None),
        };

        let calculated_addr = reg_reserve.pop().unwrap();

        if let Some(op2) = op2 {
            match op2_shift {
                Some((reg, shift_type)) => match reg {
                    Operand::Reg { reg, .. } => {
                        opcodes.push(if inst_info.op.mem_transfer_single_sub() {
                            AluReg::sub(calculated_addr, op1, op2, shift_type, reg, Cond::AL)
                        } else {
                            AluReg::add(calculated_addr, op1, op2, shift_type, reg, Cond::AL)
                        });
                    }
                    Operand::Imm(imm) => {
                        opcodes.push(if inst_info.op.mem_transfer_single_sub() {
                            AluShiftImm::sub(
                                calculated_addr,
                                op1,
                                op2,
                                shift_type,
                                imm as u8,
                                Cond::AL,
                            )
                        } else {
                            AluShiftImm::add(
                                calculated_addr,
                                op1,
                                op2,
                                shift_type,
                                imm as u8,
                                Cond::AL,
                            )
                        });
                    }
                    Operand::None => {
                        unreachable!()
                    }
                },
                None => {
                    opcodes.push(if inst_info.op.mem_transfer_single_sub() {
                        AluShiftImm::sub_al(calculated_addr, op1, op2)
                    } else {
                        AluShiftImm::add_al(calculated_addr, op1, op2)
                    });
                }
            }
        }

        let mut pop_reusable = || match reusable_regs.pop() {
            None => reg_reserve.pop().unwrap(),
            Some(reg) => reg,
        };

        if inst_info.op == Op::LdrPcT {
            opcodes.push(AluImm::bic_al(calculated_addr, calculated_addr, 3));
        }

        let addr_reg = if PRE { calculated_addr } else { op1 };

        let mmu_ptr_reg = pop_reusable();
        let physical_addr_reg = pop_reusable();

        if let Some(op) = reg_reserve.emit_push_stack(Reg::LR) {
            self.jit_buf.emit_opcodes.push(op);
        }

        // mmu_ptr = &mmu_map (*u32)
        opcodes.extend(AluImm::mov32(
            mmu_ptr_reg,
            get_mmu!(self.emu, CPU).get_mmu_ptr() as _,
        ));
        // physical_addr = addr >> 12 (u32)
        opcodes.push(AluShiftImm::mov(
            physical_addr_reg,
            addr_reg,
            ShiftType::Lsr,
            12,
            Cond::AL,
        ));
        let base_ptr_reg = mmu_ptr_reg;
        // base_ptr = *(mmu_ptr + physical_addr_reg)
        opcodes.push(LdrStrReg::ldr(
            base_ptr_reg,
            mmu_ptr_reg,
            physical_addr_reg,
            2,
            ShiftType::Lsl,
            Cond::AL,
        ));
        // Save current cpsr cond flags
        opcodes.push(Mrs::cpsr(physical_addr_reg, Cond::AL));
        // Check if mmu block is mapped
        opcodes.push(AluImm::cmp_al(base_ptr_reg, 0));

        // if base_ptr == nullptr
        let mut slow_read_opcodes = Vec::new();
        {
            static mut CALCULATED_ADDR_TMP: u32 = 0;
            unsafe {
                let tmp_ptr = ptr::addr_of_mut!(CALCULATED_ADDR_TMP) as u32;
                slow_read_opcodes.extend(AluImm::mov32(physical_addr_reg, tmp_ptr));
                slow_read_opcodes.push(LdrStrImm::str_al(calculated_addr, physical_addr_reg));
            }

            if let Some(op) = reg_reserve.clone().emit_pop_stack(Reg::LR) {
                slow_read_opcodes.push(op);
            }

            let func_addr =
                Self::get_inst_mem_handler_func::<THUMB, false, false>(inst_info.op, amount);
            slow_read_opcodes.extend(self.emit_call_host_func(
                |asm, opcodes| {
                    if PRE {
                        unsafe {
                            let tmp_ptr = ptr::addr_of_mut!(CALCULATED_ADDR_TMP) as u32;
                            opcodes.extend(AluImm::mov32(Reg::R0, tmp_ptr));
                            opcodes.push(LdrStrImm::ldr_al(Reg::R0, Reg::R0));
                        }

                        if WRITE_BACK && op0 != *og_op1 {
                            opcodes.extend(get_regs!(asm.emu, CPU).emit_set_reg(
                                *og_op1,
                                Reg::R0,
                                Reg::R1,
                            ));
                        }
                    } else {
                        if og_op1.is_emulated() || *og_op1 == Reg::SP {
                            if *og_op1 == Reg::PC {
                                opcodes
                                    .extend(AluImm::mov32(Reg::R0, pc + if THUMB { 4 } else { 8 }));
                            } else {
                                opcodes
                                    .extend(get_regs!(asm.emu, CPU).emit_get_reg(Reg::R0, *og_op1));
                            }
                        } else if *og_op1 != Reg::R0 {
                            opcodes.push(AluShiftImm::mov_al(Reg::R0, *og_op1));
                        }

                        if WRITE_BACK && op0 != *og_op1 {
                            unsafe {
                                let tmp_ptr = ptr::addr_of_mut!(CALCULATED_ADDR_TMP) as u32;
                                opcodes.extend(AluImm::mov32(Reg::R1, tmp_ptr));
                                opcodes.push(LdrStrImm::ldr_al(Reg::R1, Reg::R1));
                            }

                            opcodes.extend(get_regs!(asm.emu, CPU).emit_set_reg(
                                *og_op1,
                                Reg::R1,
                                Reg::R2,
                            ));
                        }
                    }
                },
                &[
                    None,
                    Some(get_regs!(self.emu, CPU).get_reg(op0) as *const _ as _),
                    Some(pc),
                    Some(jit_asm_addr),
                ],
                func_addr,
            ));
        }

        // if base_ptr != nullptr
        let mut mmu_read_opcodes = Vec::new();
        {
            // Restore cpsr flags
            mmu_read_opcodes.push(Msr::cpsr_flags(physical_addr_reg, Cond::AL));
            // physical_addr = addr & (1 << 12) - 1
            mmu_read_opcodes.push(AluShiftImm::mov_al(physical_addr_reg, addr_reg));
            mmu_read_opcodes.push(Bfc::create(physical_addr_reg, 12, 20, Cond::AL));
            let rd = if op0.is_emulated()
                || (WRITE_BACK && op0 == *og_op1 && amount == MemoryAmount::Word)
            {
                base_ptr_reg
            } else {
                op0
            };
            // rd = *(base_ptr + physical_addr)
            let ldr_op_fun = match amount {
                MemoryAmount::Byte => {
                    if inst_info.op.mem_transfer_single_signed() {
                        LdrStrRegSBHD::ldrsb_al
                    } else {
                        LdrStrReg::ldrb_al
                    }
                }
                MemoryAmount::Half => {
                    if inst_info.op.mem_transfer_single_signed() {
                        LdrStrRegSBHD::ldrsh_al
                    } else {
                        LdrStrRegSBHD::ldrh_al
                    }
                }
                MemoryAmount::Word => {
                    mmu_read_opcodes.push(AluImm::bic_al(physical_addr_reg, physical_addr_reg, 3));
                    LdrStrReg::ldr_al
                }
                MemoryAmount::Double => todo!(),
            };
            mmu_read_opcodes.push(ldr_op_fun(rd, base_ptr_reg, physical_addr_reg));
            if amount == MemoryAmount::Word {
                mmu_read_opcodes.push(AluShiftImm::mov(
                    physical_addr_reg,
                    addr_reg,
                    ShiftType::Lsl,
                    3,
                    Cond::AL,
                ));
                mmu_read_opcodes.push(AluReg::mov(
                    rd,
                    rd,
                    ShiftType::Ror,
                    physical_addr_reg,
                    Cond::AL,
                ));
            }

            if op0.is_emulated() {
                mmu_read_opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(
                    op0,
                    rd,
                    physical_addr_reg,
                ));
            } else if WRITE_BACK && op0 == *og_op1 && amount == MemoryAmount::Word {
                mmu_read_opcodes.push(AluShiftImm::mov_al(op0, rd));
            }

            if WRITE_BACK && op0 != *og_op1 {
                if og_op1.is_emulated() {
                    mmu_read_opcodes.extend(get_regs!(self.emu, CPU).emit_set_reg(
                        *og_op1,
                        calculated_addr,
                        physical_addr_reg,
                    ))
                } else {
                    mmu_read_opcodes.push(AluShiftImm::mov_al(*og_op1, calculated_addr));
                }
            }

            if let Some(op) = reg_reserve.clone().emit_pop_stack(Reg::LR) {
                mmu_read_opcodes.push(op);
            }
        }

        self.jit_buf.emit_opcodes.extend(opcodes);
        self.jit_buf
            .emit_opcodes
            .push(B::b(mmu_read_opcodes.len() as i32, Cond::EQ));
        self.jit_buf.emit_opcodes.extend(mmu_read_opcodes);
        self.jit_buf
            .emit_opcodes
            .push(B::b((slow_read_opcodes.len() - 1) as i32, Cond::AL));
        self.jit_buf.emit_opcodes.extend(slow_read_opcodes);
    }

    pub fn emit_multiple_transfer<const THUMB: bool>(&mut self, buf_index: usize, pc: u32) {
        let jit_asm_addr = self as *mut _ as _;
        let inst_info = &self.jit_buf.instructions[buf_index];

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

        #[rustfmt::skip]
        let func_addr = match (
            inst_info.op.mem_is_write(),
            inst_info.op.mem_transfer_user(),
            pre,
            write_back,
            decrement,
            has_pc,
        ) {
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

    pub fn emit_swp(&mut self, buf_index: usize, pc: u32) {
        let jit_asm_addr = self as *mut _ as _;
        let inst_info = &self.jit_buf.instructions[buf_index];
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

        self.jit_buf.emit_opcodes.extend(self.emit_call_host_func(
            |_, _| {},
            &[Some(reg_arg), Some(pc), Some(jit_asm_addr)],
            func_addr,
        ));
    }
}

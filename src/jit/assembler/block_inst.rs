use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg, AluShiftImm, Bfc};
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::assembler::arm::transfer_assembler::{LdrStrImm, LdrStrImmSBHD, LdrStrReg, LdrStrRegSBHD, Mrs, Msr};
use crate::jit::assembler::block_reg_set::{block_reg_set, BlockRegSet};
use crate::jit::assembler::{BlockLabel, BlockOperand, BlockOperandShift, BlockReg, BlockShift};
use crate::jit::reg::Reg;
use crate::jit::{Cond, MemoryAmount, ShiftType};

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BlockAluOp {
    And = 0,
    Eor = 1,
    Sub = 2,
    Rsb = 3,
    Add = 4,
    Adc = 5,
    Sbc = 6,
    Rsc = 7,
    Tst = 8,
    Teq = 9,
    Cmp = 10,
    Cmn = 11,
    Orr = 12,
    Mov = 13,
    Bic = 14,
    Mvn = 15,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BlockTransferOp {
    Read,
    Write,
}

#[derive(Copy, Clone, Debug)]
pub enum BlockSystemRegOp {
    Mrs,
    Msr,
}

impl BlockInst {
    pub fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        match self {
            BlockInst::Alu3 { operands, .. } => (
                block_reg_set!(Some(operands[1].as_reg()), operands[2].try_as_reg(), operands[2].try_as_shift_reg()),
                block_reg_set!(Some(operands[0].as_reg())),
            ),
            BlockInst::Alu2Op1 { operands, .. } => (
                block_reg_set!(Some(operands[0].as_reg()), operands[1].try_as_reg(), operands[1].try_as_shift_reg()),
                block_reg_set!(Some(BlockReg::Guest(Reg::CPSR))),
            ),
            BlockInst::Alu2Op0 { operands, .. } => (block_reg_set!(operands[1].try_as_reg(), operands[1].try_as_shift_reg()), block_reg_set!(Some(operands[0].as_reg()))),
            BlockInst::Transfer { op, operands, .. } => match op {
                BlockTransferOp::Read => (
                    block_reg_set!(Some(operands[1].as_reg()), operands[2].try_as_reg(), operands[2].try_as_shift_reg()),
                    block_reg_set!(Some(operands[0].as_reg())),
                ),
                BlockTransferOp::Write => (
                    block_reg_set!(Some(operands[0].as_reg()), Some(operands[1].as_reg()), operands[2].try_as_reg(), operands[2].try_as_shift_reg()),
                    block_reg_set!(),
                ),
            },
            BlockInst::SystemReg { op, operand } => match op {
                BlockSystemRegOp::Mrs => (block_reg_set!(), block_reg_set!(Some(operand.as_reg()))),
                BlockSystemRegOp::Msr => (block_reg_set!(operand.try_as_reg()), block_reg_set!()),
            },
            BlockInst::Bfc { operand, .. } => (block_reg_set!(Some(*operand)), block_reg_set!(Some(*operand))),
            BlockInst::Label(_) => (block_reg_set!(), block_reg_set!()),
            BlockInst::Branch { .. } => (block_reg_set!(), block_reg_set!()),
            BlockInst::SaveContext {
                thread_regs_addr_reg,
                tmp_guest_cpsr_reg,
                regs_to_save,
                ..
            } => {
                let mut inputs = BlockRegSet::new();
                for reg_to_save in regs_to_save {
                    if let Some(reg_to_save) = reg_to_save {
                        inputs += *reg_to_save;
                    }
                }
                if !inputs.is_empty() {
                    inputs += *thread_regs_addr_reg;
                }
                let mut outputs = BlockRegSet::new();
                if let Some(reg) = regs_to_save[Reg::CPSR as usize] {
                    outputs += reg;
                    outputs += *tmp_guest_cpsr_reg;
                }
                (inputs, outputs)
            }
            BlockInst::Call { func_reg, args } => {
                let mut inputs = BlockRegSet::new();
                inputs += *func_reg;
                for arg in args {
                    if let Some(arg) = arg {
                        if let Ok(arg) = TryInto::<BlockReg>::try_into(*arg) {
                            inputs += arg;
                        }
                    }
                }
                (
                    inputs,
                    block_reg_set!(
                        Some(BlockReg::Fixed(Reg::R0)),
                        Some(BlockReg::Fixed(Reg::R1)),
                        Some(BlockReg::Fixed(Reg::R2)),
                        Some(BlockReg::Fixed(Reg::R3)),
                        Some(BlockReg::Fixed(Reg::R12))
                    ),
                )
            }
        }
    }

    pub fn replace_regs(&mut self, old: BlockReg, new: BlockReg) {
        let replace_reg = |reg: &mut BlockReg| {
            if *reg == old {
                *reg = new;
            }
        };
        let replace_operand = |operand: &mut BlockOperand| {
            if let BlockOperand::Reg(reg) = operand {
                if *reg == old {
                    *reg = new;
                }
            }
        };
        let replace_shift_operands = |operands: &mut [BlockOperandShift]| {
            for operand in operands {
                operand.replace_regs(old, new);
            }
        };
        match self {
            BlockInst::Alu3 { operands, .. } => replace_shift_operands(operands),
            BlockInst::Alu2Op1 { operands, .. } => replace_shift_operands(operands),
            BlockInst::Alu2Op0 { operands, .. } => replace_shift_operands(operands),
            BlockInst::Transfer { operands, .. } => replace_shift_operands(operands),
            BlockInst::SystemReg { operand, .. } => replace_operand(operand),
            BlockInst::Bfc { operand, .. } => replace_reg(operand),
            BlockInst::Label(_) => {}
            BlockInst::Branch { .. } => {}
            BlockInst::SaveContext {
                thread_regs_addr_reg,
                tmp_guest_cpsr_reg,
                regs_to_save,
            } => {
                replace_reg(thread_regs_addr_reg);
                replace_reg(tmp_guest_cpsr_reg);
                for reg in regs_to_save {
                    if let Some(reg) = reg {
                        replace_reg(reg);
                    }
                }
            }
            BlockInst::Call { func_reg, .. } => replace_reg(func_reg),
        }
    }

    pub fn emit_opcode(&self, opcodes: &mut Vec<u32>, opcode_index: usize, branch_placeholders: &mut Vec<usize>, opcodes_offset: usize) {
        let alu_reg = |op: BlockAluOp, op0: BlockReg, op1: BlockReg, op2: BlockReg, shift: BlockShift, set_cond: bool| match shift.value {
            BlockOperand::Reg(shift_reg) => AluReg::generic(op as u8, op0.as_fixed(), op1.as_fixed(), op2.as_fixed(), shift.shift_type, shift_reg.as_fixed(), set_cond, Cond::AL),
            BlockOperand::Imm(shift_imm) => {
                assert_eq!(shift_imm & !0x1F, 0);
                AluShiftImm::generic(op as u8, op0.as_fixed(), op1.as_fixed(), op2.as_fixed(), shift.shift_type, shift_imm as u8, set_cond, Cond::AL)
            }
        };
        let alu_imm = |op: BlockAluOp, op0: BlockReg, op1: BlockReg, op2: u32, set_cond: bool| {
            assert_eq!(op2 & !0xFF, 0);
            AluImm::generic(op as u8, op0.as_fixed(), op1.as_fixed(), op2 as u8, 0, set_cond, Cond::AL)
        };

        match self {
            BlockInst::Alu3 { op, operands } => match operands[2].operand {
                BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, operands[0].as_reg(), operands[1].as_reg(), reg, operands[2].shift, false)),
                BlockOperand::Imm(imm) => opcodes.push(alu_imm(*op, operands[0].as_reg(), operands[1].as_reg(), imm, false)),
            },
            BlockInst::Alu2Op1 { op, operands } => match operands[1].operand {
                BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, BlockReg::Fixed(Reg::R0), operands[0].as_reg(), reg, operands[1].shift, true)),
                BlockOperand::Imm(imm) => opcodes.push(alu_imm(*op, BlockReg::Fixed(Reg::R0), operands[0].as_reg(), imm, true)),
            },
            BlockInst::Alu2Op0 { op, operands } => match operands[1].operand {
                BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, operands[0].as_reg(), BlockReg::Fixed(Reg::R0), reg, operands[1].shift, false)),
                BlockOperand::Imm(imm) => {
                    if *op == BlockAluOp::Mov {
                        opcodes.extend(AluImm::mov32(operands[0].as_reg().as_fixed(), imm))
                    } else {
                        opcodes.push(alu_imm(*op, operands[0].as_reg(), BlockReg::Fixed(Reg::R0), imm, false))
                    }
                }
            },
            BlockInst::Transfer { op, operands, signed, amount } => opcodes.push(match operands[2].operand {
                BlockOperand::Reg(reg) => {
                    let func = match amount {
                        MemoryAmount::Byte => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, signed: bool, read: bool, cond: Cond| {
                            if signed {
                                assert_eq!(shift_amount, 0);
                                LdrStrRegSBHD::generic(op0, op1, op2, true, MemoryAmount::Byte, read, false, true, true, cond)
                            } else {
                                LdrStrReg::generic(op0, op1, op2, shift_amount, shift_type, read, false, true, true, true, cond)
                            }
                        },
                        MemoryAmount::Half => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, _: ShiftType, signed: bool, read: bool, cond: Cond| {
                            assert_eq!(shift_amount, 0);
                            LdrStrRegSBHD::generic(op0, op1, op2, signed, MemoryAmount::Half, read, false, true, true, cond)
                        },
                        MemoryAmount::Word => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, signed: bool, read: bool, cond: Cond| {
                            assert!(!signed);
                            LdrStrReg::generic(op0, op1, op2, shift_amount, shift_type, read, false, false, true, true, cond)
                        },
                        MemoryAmount::Double => {
                            todo!()
                        }
                    };
                    let shift = operands[2].as_shift_imm();
                    assert_eq!(shift & !0x1F, 0);
                    func(
                        operands[0].as_reg().as_fixed(),
                        operands[1].as_reg().as_fixed(),
                        reg.as_fixed(),
                        shift as u8,
                        operands[2].shift.shift_type,
                        *signed,
                        *op == BlockTransferOp::Read,
                        Cond::AL,
                    )
                }
                BlockOperand::Imm(imm) => {
                    let func = match amount {
                        MemoryAmount::Byte => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, cond: Cond| {
                            if signed {
                                assert_eq!(imm_offset & !0xFF, 0);
                                LdrStrImmSBHD::generic(op0, op1, imm_offset as u8, true, MemoryAmount::Byte, read, false, true, true, cond)
                            } else {
                                LdrStrImm::generic(op0, op1, imm_offset, read, false, true, true, true, cond)
                            }
                        },
                        MemoryAmount::Half => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, cond: Cond| {
                            assert_eq!(imm_offset & !0xFF, 0);
                            LdrStrImmSBHD::generic(op0, op1, imm_offset as u8, signed, MemoryAmount::Half, read, false, true, true, cond)
                        },
                        MemoryAmount::Word => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, cond: Cond| {
                            assert!(!signed);
                            LdrStrImm::generic(op0, op1, imm_offset, read, false, false, true, true, cond)
                        },
                        MemoryAmount::Double => {
                            todo!()
                        }
                    };
                    func(
                        operands[0].as_reg().as_fixed(),
                        operands[1].as_reg().as_fixed(),
                        imm as u16,
                        *signed,
                        *op == BlockTransferOp::Read,
                        Cond::AL,
                    )
                }
            }),
            BlockInst::SystemReg { op, operand } => match op {
                BlockSystemRegOp::Mrs => opcodes.push(Mrs::cpsr(operand.as_reg().as_fixed(), Cond::AL)),
                BlockSystemRegOp::Msr => opcodes.push(Msr::cpsr_flags(operand.as_reg().as_fixed(), Cond::AL)),
            },
            BlockInst::Bfc { operand, lsb, width } => opcodes.push(Bfc::create(operand.as_fixed(), *lsb, *width, Cond::AL)),

            BlockInst::Label(_) => {}
            BlockInst::Branch { cond, block_index, .. } => {
                // Encode label and cond as u32
                // Branch offset can only be figured out later
                opcodes.push(((*cond as u32) << 28) | (*block_index as u32));
                branch_placeholders.push(opcodes_offset + opcode_index);
            }

            BlockInst::SaveContext {
                thread_regs_addr_reg,
                tmp_guest_cpsr_reg,
                regs_to_save,
            } => {
                for (i, reg) in regs_to_save[..Reg::CPSR as usize].iter().enumerate() {
                    if let Some(reg) = reg {
                        opcodes.push(LdrStrImm::str_offset_al(reg.as_fixed(), thread_regs_addr_reg.as_fixed(), i as u16 * 4));
                    }
                }
                if let Some(reg) = regs_to_save[Reg::CPSR as usize] {
                    opcodes.push(LdrStrImm::ldr_offset_al(tmp_guest_cpsr_reg.as_fixed(), thread_regs_addr_reg.as_fixed(), Reg::CPSR as u16 * 4));
                    // Only copy the cond flags from host cpsr
                    opcodes.push(AluImm::and(
                        reg.as_fixed(),
                        reg.as_fixed(),
                        0xF8,
                        4, // 8 Bytes, steps of 2
                        Cond::AL,
                    ));
                    opcodes.push(AluImm::bic(
                        tmp_guest_cpsr_reg.as_fixed(),
                        tmp_guest_cpsr_reg.as_fixed(),
                        0xF8,
                        4, // 8 Bytes, steps of 2
                        Cond::AL,
                    ));
                    opcodes.push(AluShiftImm::orr_al(tmp_guest_cpsr_reg.as_fixed(), reg.as_fixed(), tmp_guest_cpsr_reg.as_fixed()));
                    opcodes.push(LdrStrImm::str_offset_al(tmp_guest_cpsr_reg.as_fixed(), thread_regs_addr_reg.as_fixed(), Reg::CPSR as u16 * 4));
                }
            }
            BlockInst::Call { func_reg, .. } => opcodes.push(Bx::blx(func_reg.as_fixed(), Cond::AL)),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum BlockInst {
    Alu3 {
        op: BlockAluOp,
        operands: [BlockOperandShift; 3],
    },
    Alu2Op1 {
        op: BlockAluOp,
        operands: [BlockOperandShift; 2],
    },
    Alu2Op0 {
        op: BlockAluOp,
        operands: [BlockOperandShift; 2],
    },
    Transfer {
        op: BlockTransferOp,
        operands: [BlockOperandShift; 3],
        signed: bool,
        amount: MemoryAmount,
    },
    SystemReg {
        op: BlockSystemRegOp,
        operand: BlockOperand,
    },
    Bfc {
        operand: BlockReg,
        lsb: u8,
        width: u8,
    },

    Label(BlockLabel),
    Branch {
        label: BlockLabel,
        cond: Cond,
        block_index: usize,
    },

    SaveContext {
        thread_regs_addr_reg: BlockReg,
        tmp_guest_cpsr_reg: BlockReg,
        regs_to_save: [Option<BlockReg>; Reg::SPSR as usize],
    },
    Call {
        func_reg: BlockReg,
        args: [Option<BlockReg>; 4],
    },
}

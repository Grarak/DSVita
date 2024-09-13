use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg, AluShiftImm, Bfc, MulReg};
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, LdrStrImmSBHD, LdrStrReg, LdrStrRegSBHD, Mrs, Msr};
use crate::jit::assembler::arm::Bkpt;
use crate::jit::assembler::block_reg_set::{block_reg_set, BlockRegSet};
use crate::jit::assembler::{BlockLabel, BlockOperand, BlockOperandShift, BlockReg, BlockShift};
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, ShiftType};
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockAluSetCond {
    None,
    Host,
    HostGuest,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BlockTransferOp {
    Read,
    Write,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BlockSystemRegOp {
    Mrs,
    Msr,
}

impl BlockInst {
    pub fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        match self {
            BlockInst::Alu3 { operands, set_cond, .. } | BlockInst::Mul { operands, set_cond, .. } => {
                let mut outputs = BlockRegSet::new();
                outputs += operands[0].as_reg();
                match set_cond {
                    BlockAluSetCond::Host => outputs += BlockReg::Fixed(Reg::CPSR),
                    BlockAluSetCond::HostGuest => {
                        outputs += BlockReg::from(Reg::CPSR);
                        outputs += BlockReg::Fixed(Reg::CPSR);
                    }
                    _ => {}
                }
                (block_reg_set!(Some(operands[1].as_reg()), operands[2].try_as_reg(), operands[2].try_as_shift_reg()), outputs)
            }
            BlockInst::Alu2Op1 { operands, set_cond, .. } => {
                let mut outputs = BlockRegSet::new();
                match set_cond {
                    BlockAluSetCond::Host => outputs += BlockReg::Fixed(Reg::CPSR),
                    BlockAluSetCond::HostGuest => {
                        outputs += BlockReg::from(Reg::CPSR);
                        outputs += BlockReg::Fixed(Reg::CPSR);
                    }
                    _ => panic!(),
                }
                (block_reg_set!(Some(operands[0].as_reg()), operands[1].try_as_reg(), operands[1].try_as_shift_reg()), outputs)
            }
            BlockInst::Alu2Op0 { operands, set_cond, .. } => {
                let mut outputs = BlockRegSet::new();
                outputs += operands[0].as_reg();
                match set_cond {
                    BlockAluSetCond::Host => outputs += BlockReg::Fixed(Reg::CPSR),
                    BlockAluSetCond::HostGuest => {
                        outputs += BlockReg::from(Reg::CPSR);
                        outputs += BlockReg::Fixed(Reg::CPSR)
                    }
                    _ => {}
                }
                (block_reg_set!(operands[1].try_as_reg(), operands[1].try_as_shift_reg()), outputs)
            }
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
            BlockInst::TransferMultiple { op, operand, regs, write_back, .. } => match op {
                BlockTransferOp::Read => (
                    block_reg_set!(Some(*operand)),
                    if *write_back { BlockRegSet::new_fixed(*regs) + *operand } else { BlockRegSet::new_fixed(*regs) },
                ),
                BlockTransferOp::Write => (BlockRegSet::new_fixed(*regs) + *operand, if *write_back { block_reg_set!(Some(*operand)) } else { block_reg_set!() }),
            },
            BlockInst::SystemReg { op, operand } => match op {
                BlockSystemRegOp::Mrs => (block_reg_set!(), block_reg_set!(Some(operand.as_reg()))),
                BlockSystemRegOp::Msr => (block_reg_set!(operand.try_as_reg()), block_reg_set!()),
            },
            BlockInst::Bfc { operand, .. } => (block_reg_set!(Some(*operand)), block_reg_set!(Some(*operand))),

            BlockInst::SaveContext { .. } => {
                unreachable!()
            }
            BlockInst::SaveReg {
                guest_reg,
                reg_mapped,
                thread_regs_addr_reg,
                tmp_guest_cpsr_reg,
            } => {
                let mut inputs = BlockRegSet::new();
                let mut outputs = BlockRegSet::new();
                match guest_reg {
                    Reg::CPSR => {
                        inputs += BlockReg::Fixed(Reg::CPSR);
                        inputs += *thread_regs_addr_reg;
                        outputs += *reg_mapped;
                        outputs += *tmp_guest_cpsr_reg;
                    }
                    _ => {
                        inputs += *reg_mapped;
                        inputs += *thread_regs_addr_reg;
                    }
                }
                (inputs, outputs)
            }
            BlockInst::RestoreReg {
                guest_reg,
                reg_mapped,
                thread_regs_addr_reg,
                tmp_guest_cpsr_reg,
            } => {
                let mut outputs = BlockRegSet::new();
                outputs += *reg_mapped;
                if *guest_reg == Reg::CPSR {
                    outputs += *tmp_guest_cpsr_reg;
                    outputs += BlockReg::Fixed(Reg::CPSR);
                }
                (block_reg_set!(Some(*thread_regs_addr_reg)), outputs)
            }

            BlockInst::Call { func_reg, args } => {
                let mut inputs = BlockRegSet::new();
                inputs += *func_reg;
                for arg in args {
                    if let Some(arg) = arg {
                        inputs += *arg;
                    }
                }
                (
                    inputs,
                    block_reg_set!(
                        Some(BlockReg::Fixed(Reg::R0)),
                        Some(BlockReg::Fixed(Reg::R1)),
                        Some(BlockReg::Fixed(Reg::R2)),
                        Some(BlockReg::Fixed(Reg::R3)),
                        Some(BlockReg::Fixed(Reg::R12)),
                        Some(BlockReg::Fixed(Reg::CPSR))
                    ),
                )
            }
            BlockInst::GenericGuestInst { inst, regs_mapping } => {
                let mut inputs = BlockRegSet::new();
                let mut outputs = BlockRegSet::new();
                for reg in inst.src_regs {
                    inputs += regs_mapping[reg as usize];
                }
                for reg in inst.out_regs {
                    outputs += regs_mapping[reg as usize];
                }
                (inputs, outputs)
            }
            BlockInst::Label { .. } | BlockInst::Branch { .. } | BlockInst::GuestPc(_) | BlockInst::Bkpt(_) => (block_reg_set!(), block_reg_set!()),
        }
    }

    fn replace_reg(reg: &mut BlockReg, old: BlockReg, new: BlockReg) {
        if *reg == old {
            *reg = new;
        }
    }

    fn replace_operand(operand: &mut BlockOperand, old: BlockReg, new: BlockReg) {
        if let BlockOperand::Reg(reg) = operand {
            if *reg == old {
                *reg = new;
            }
        }
    }

    fn replace_shift_operands(operands: &mut [BlockOperandShift], old: BlockReg, new: BlockReg) {
        for operand in operands {
            operand.replace_regs(old, new);
        }
    }

    pub fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        match self {
            BlockInst::Alu3 { operands, .. } | BlockInst::Mul { operands, .. } => {
                operands[1].replace_regs(old, new);
                operands[2].replace_regs(old, new);
            }
            BlockInst::Alu2Op1 { operands, .. } => Self::replace_shift_operands(operands, old, new),
            BlockInst::Alu2Op0 { operands, .. } => operands[1].replace_regs(old, new),
            BlockInst::Transfer { op, operands, .. } => {
                if *op == BlockTransferOp::Write {
                    operands[0].replace_regs(old, new);
                }
                operands[1].replace_regs(old, new);
                operands[2].replace_regs(old, new);
            }
            BlockInst::TransferMultiple { operand, .. } => Self::replace_reg(operand, old, new),
            BlockInst::SystemReg { op, operand } => {
                if *op == BlockSystemRegOp::Msr {
                    Self::replace_operand(operand, old, new);
                }
            }
            BlockInst::Bfc { operand, .. } => Self::replace_reg(operand, old, new),
            BlockInst::SaveContext { .. } => {
                unreachable!()
            }
            BlockInst::SaveReg {
                guest_reg,
                reg_mapped,
                thread_regs_addr_reg,
                ..
            } => {
                if *guest_reg != Reg::CPSR {
                    Self::replace_reg(reg_mapped, old, new);
                }
                Self::replace_reg(thread_regs_addr_reg, old, new);
            }
            BlockInst::RestoreReg { thread_regs_addr_reg, .. } => Self::replace_reg(thread_regs_addr_reg, old, new),
            BlockInst::Call { func_reg, .. } => Self::replace_reg(func_reg, old, new),
            BlockInst::GenericGuestInst { inst, regs_mapping } => {
                for reg in inst.src_regs {
                    Self::replace_reg(&mut regs_mapping[reg as usize], old, new);
                }
            }
            BlockInst::Label { .. } | BlockInst::Branch { .. } | BlockInst::GuestPc(_) | BlockInst::Bkpt(_) => {}
        }
    }

    pub fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        match self {
            BlockInst::Alu3 { operands, .. } | BlockInst::Mul { operands, .. } => operands[0].replace_regs(old, new),
            BlockInst::Alu2Op1 { .. } => {}
            BlockInst::Alu2Op0 { operands, .. } => operands[0].replace_regs(old, new),
            BlockInst::Transfer { op, operands, .. } => {
                if *op == BlockTransferOp::Read {
                    operands[0].replace_regs(old, new);
                }
            }
            BlockInst::TransferMultiple { operand, write_back, .. } => {
                if *write_back {
                    Self::replace_reg(operand, old, new);
                }
            }
            BlockInst::SystemReg { op, operand } => {
                if *op == BlockSystemRegOp::Mrs {
                    Self::replace_operand(operand, old, new);
                }
            }
            BlockInst::Bfc { operand, .. } => Self::replace_reg(operand, old, new),
            BlockInst::SaveContext { tmp_guest_cpsr_reg, .. } => Self::replace_reg(tmp_guest_cpsr_reg, old, new),
            BlockInst::SaveReg {
                guest_reg,
                reg_mapped,
                tmp_guest_cpsr_reg,
                ..
            } => {
                if *guest_reg == Reg::CPSR {
                    Self::replace_reg(reg_mapped, old, new);
                }
                Self::replace_reg(tmp_guest_cpsr_reg, old, new)
            }
            BlockInst::RestoreReg {
                guest_reg,
                reg_mapped,
                tmp_guest_cpsr_reg,
                ..
            } => {
                Self::replace_reg(reg_mapped, old, new);
                if *guest_reg == Reg::CPSR {
                    Self::replace_reg(tmp_guest_cpsr_reg, old, new);
                }
            }
            BlockInst::Call { .. } => {}
            BlockInst::GenericGuestInst { inst, regs_mapping } => {
                for reg in inst.out_regs {
                    Self::replace_reg(&mut regs_mapping[reg as usize], old, new);
                }
            }
            BlockInst::Label { .. } | BlockInst::Branch { .. } | BlockInst::GuestPc(_) | BlockInst::Bkpt(_) => {}
        }
    }

    pub fn replace_regs(&mut self, old: BlockReg, new: BlockReg) {
        match self {
            BlockInst::Alu3 { operands, .. } | BlockInst::Mul { operands, .. } => Self::replace_shift_operands(operands, old, new),
            BlockInst::Alu2Op1 { operands, .. } => Self::replace_shift_operands(operands, old, new),
            BlockInst::Alu2Op0 { operands, .. } => Self::replace_shift_operands(operands, old, new),
            BlockInst::Transfer { operands, .. } => Self::replace_shift_operands(operands, old, new),
            BlockInst::TransferMultiple { operand, .. } => Self::replace_reg(operand, old, new),
            BlockInst::SystemReg { operand, .. } => Self::replace_operand(operand, old, new),
            BlockInst::Bfc { operand, .. } => Self::replace_reg(operand, old, new),

            BlockInst::SaveContext { .. } => {
                unreachable!()
            }
            BlockInst::SaveReg {
                reg_mapped,
                thread_regs_addr_reg,
                tmp_guest_cpsr_reg,
                ..
            }
            | BlockInst::RestoreReg {
                reg_mapped,
                thread_regs_addr_reg,
                tmp_guest_cpsr_reg,
                ..
            } => {
                Self::replace_reg(reg_mapped, old, new);
                Self::replace_reg(thread_regs_addr_reg, old, new);
                Self::replace_reg(tmp_guest_cpsr_reg, old, new);
            }

            BlockInst::Call { func_reg, .. } => Self::replace_reg(func_reg, old, new),
            BlockInst::GenericGuestInst { regs_mapping, .. } => {
                for reg_mapping in regs_mapping {
                    Self::replace_reg(reg_mapping, old, new);
                }
            }
            BlockInst::Label { .. } | BlockInst::Branch { .. } | BlockInst::GuestPc(_) | BlockInst::Bkpt(_) => {}
        }
    }

    fn save_guest_cpsr(opcodes: &mut Vec<u32>, thread_regs_addr_reg: Reg, host_reg: Reg, guest_reg: Reg) {
        opcodes.push(Mrs::cpsr(host_reg, Cond::AL));
        opcodes.push(LdrStrImm::ldr_offset_al(guest_reg, thread_regs_addr_reg, Reg::CPSR as u16 * 4));
        // Only copy the cond flags from host cpsr
        opcodes.push(AluImm::and(
            host_reg,
            host_reg,
            0xF8,
            4, // 8 Bytes, steps of 2
            Cond::AL,
        ));
        opcodes.push(AluImm::bic(
            guest_reg,
            guest_reg,
            0xF8,
            4, // 8 Bytes, steps of 2
            Cond::AL,
        ));
        opcodes.push(AluShiftImm::orr_al(guest_reg, host_reg, guest_reg));
        opcodes.push(LdrStrImm::str_offset_al(guest_reg, thread_regs_addr_reg, Reg::CPSR as u16 * 4));
    }

    pub fn emit_opcode(&mut self, opcodes: &mut Vec<u32>, opcode_index: usize, branch_placeholders: &mut Vec<usize>, opcodes_offset: usize) {
        let alu_reg = |op: BlockAluOp, op0: BlockReg, op1: BlockReg, op2: BlockReg, shift: BlockShift, set_cond: bool| match shift.value {
            BlockOperand::Reg(shift_reg) => AluReg::generic(op as u8, op0.as_fixed(), op1.as_fixed(), op2.as_fixed(), shift.shift_type, shift_reg.as_fixed(), set_cond, Cond::AL),
            BlockOperand::Imm(shift_imm) => {
                assert_eq!(shift_imm & !0x1F, 0);
                AluShiftImm::generic(op as u8, op0.as_fixed(), op1.as_fixed(), op2.as_fixed(), shift.shift_type, shift_imm as u8, set_cond, Cond::AL)
            }
        };
        let alu_imm = |op: BlockAluOp, op0: BlockReg, op1: BlockReg, op2: u32, shift: BlockShift, set_cond: bool| {
            assert_eq!(op2 & !0xFF, 0);
            let shift_value = shift.value.as_imm();
            assert_eq!(shift_value & !0xF, 0);
            assert!(shift_value == 0 || shift.shift_type == ShiftType::Ror);
            AluImm::generic(op as u8, op0.as_fixed(), op1.as_fixed(), op2 as u8, shift_value as u8, set_cond, Cond::AL)
        };

        match self {
            BlockInst::Alu3 { op, operands, set_cond, .. } => match operands[2].operand {
                BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, operands[0].as_reg(), operands[1].as_reg(), reg, operands[2].shift, *set_cond != BlockAluSetCond::None)),
                BlockOperand::Imm(imm) => opcodes.push(alu_imm(*op, operands[0].as_reg(), operands[1].as_reg(), imm, operands[2].shift, *set_cond != BlockAluSetCond::None)),
            },
            BlockInst::Alu2Op1 { op, operands, set_cond, .. } => {
                assert_ne!(*set_cond, BlockAluSetCond::None);
                match operands[1].operand {
                    BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, BlockReg::Fixed(Reg::R0), operands[0].as_reg(), reg, operands[1].shift, true)),
                    BlockOperand::Imm(imm) => opcodes.push(alu_imm(*op, BlockReg::Fixed(Reg::R0), operands[0].as_reg(), imm, operands[1].shift, true)),
                }
            }
            BlockInst::Alu2Op0 { op, operands, set_cond, .. } => match operands[1].operand {
                BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, operands[0].as_reg(), BlockReg::Fixed(Reg::R0), reg, operands[1].shift, *set_cond != BlockAluSetCond::None)),
                BlockOperand::Imm(imm) => {
                    if *op == BlockAluOp::Mov && *set_cond == BlockAluSetCond::None {
                        opcodes.extend(AluImm::mov32(operands[0].as_reg().as_fixed(), imm))
                    } else {
                        opcodes.push(alu_imm(*op, operands[0].as_reg(), BlockReg::Fixed(Reg::R0), imm, operands[1].shift, *set_cond != BlockAluSetCond::None))
                    }
                }
            },
            BlockInst::Transfer {
                op,
                operands,
                signed,
                amount,
                add_to_base,
            } => opcodes.push(match operands[2].operand {
                BlockOperand::Reg(reg) => {
                    let func = match amount {
                        MemoryAmount::Byte => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            if signed {
                                assert_eq!(shift_amount, 0);
                                LdrStrRegSBHD::generic(op0, op1, op2, true, MemoryAmount::Byte, read, false, add_to_base, true, cond)
                            } else {
                                LdrStrReg::generic(op0, op1, op2, shift_amount, shift_type, read, false, true, add_to_base, true, cond)
                            }
                        },
                        MemoryAmount::Half => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, _: ShiftType, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            assert_eq!(shift_amount, 0);
                            LdrStrRegSBHD::generic(op0, op1, op2, signed, MemoryAmount::Half, read, false, add_to_base, true, cond)
                        },
                        MemoryAmount::Word => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            assert!(!signed);
                            LdrStrReg::generic(op0, op1, op2, shift_amount, shift_type, read, false, false, add_to_base, true, cond)
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
                        *add_to_base,
                        Cond::AL,
                    )
                }
                BlockOperand::Imm(imm) => {
                    let func = match amount {
                        MemoryAmount::Byte => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            if signed {
                                assert_eq!(imm_offset & !0xFF, 0);
                                LdrStrImmSBHD::generic(op0, op1, imm_offset as u8, true, MemoryAmount::Byte, read, false, true, true, cond)
                            } else {
                                LdrStrImm::generic(op0, op1, imm_offset, read, false, true, add_to_base, true, cond)
                            }
                        },
                        MemoryAmount::Half => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            assert_eq!(imm_offset & !0xFF, 0);
                            LdrStrImmSBHD::generic(op0, op1, imm_offset as u8, signed, MemoryAmount::Half, read, false, add_to_base, true, cond)
                        },
                        MemoryAmount::Word => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            assert!(!signed);
                            LdrStrImm::generic(op0, op1, imm_offset, read, false, false, add_to_base, true, cond)
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
                        *add_to_base,
                        Cond::AL,
                    )
                }
            }),
            BlockInst::TransferMultiple {
                op,
                operand,
                regs,
                write_back,
                pre,
                add_to_base,
            } => opcodes.push(LdmStm::generic(operand.as_fixed(), *regs, *op == BlockTransferOp::Read, *write_back, *add_to_base, *pre, Cond::AL)),
            BlockInst::SystemReg { op, operand } => match op {
                BlockSystemRegOp::Mrs => opcodes.push(Mrs::cpsr(operand.as_reg().as_fixed(), Cond::AL)),
                BlockSystemRegOp::Msr => opcodes.push(Msr::cpsr_flags(operand.as_reg().as_fixed(), Cond::AL)),
            },
            BlockInst::Bfc { operand, lsb, width } => opcodes.push(Bfc::create(operand.as_fixed(), *lsb, *width, Cond::AL)),
            BlockInst::Mul { operands, set_cond, .. } => match operands[2].operand {
                BlockOperand::Reg(reg) => opcodes.push(MulReg::mul(
                    operands[0].as_reg().as_fixed(),
                    operands[1].as_reg().as_fixed(),
                    reg.as_fixed(),
                    *set_cond != BlockAluSetCond::None,
                    Cond::AL,
                )),
                BlockOperand::Imm(_) => {
                    todo!()
                }
            },

            BlockInst::Branch { cond, block_index, skip, .. } => {
                if !*skip {
                    // Encode label and cond as u32
                    // Branch offset can only be figured out later
                    opcodes.push(((*cond as u32) << 28) | (*block_index as u32));
                    branch_placeholders.push(opcodes_offset + opcode_index);
                }
            }

            BlockInst::SaveContext { .. } => {
                unreachable!()
            }
            BlockInst::SaveReg {
                guest_reg,
                reg_mapped,
                thread_regs_addr_reg,
                tmp_guest_cpsr_reg,
            } => match guest_reg {
                Reg::CPSR => Self::save_guest_cpsr(opcodes, thread_regs_addr_reg.as_fixed(), reg_mapped.as_fixed(), tmp_guest_cpsr_reg.as_fixed()),
                _ => opcodes.push(LdrStrImm::str_offset_al(reg_mapped.as_fixed(), thread_regs_addr_reg.as_fixed(), *guest_reg as u16 * 4)),
            },
            BlockInst::RestoreReg {
                guest_reg,
                reg_mapped,
                thread_regs_addr_reg,
                tmp_guest_cpsr_reg,
            } => match guest_reg {
                Reg::CPSR => {
                    opcodes.push(LdrStrImm::ldr_offset_al(tmp_guest_cpsr_reg.as_fixed(), thread_regs_addr_reg.as_fixed(), Reg::CPSR as u16 * 4));
                    opcodes.push(Msr::cpsr_flags(tmp_guest_cpsr_reg.as_fixed(), Cond::AL));
                }
                _ => opcodes.push(LdrStrImm::ldr_offset_al(reg_mapped.as_fixed(), thread_regs_addr_reg.as_fixed(), *guest_reg as u16 * 4)),
            },

            BlockInst::Call { func_reg, .. } => opcodes.push(Bx::blx(func_reg.as_fixed(), Cond::AL)),
            BlockInst::Bkpt(id) => opcodes.push(Bkpt::bkpt(*id)),

            BlockInst::GenericGuestInst { inst, regs_mapping } => {
                let replace_reg = |reg: &mut Reg| {
                    *reg = regs_mapping[*reg as usize].as_fixed();
                };
                let replace_shift = |shift_value: &mut ShiftValue| {
                    if let ShiftValue::Reg(reg) = shift_value {
                        replace_reg(reg);
                    }
                };

                let inst_info = inst.deref_mut();
                for operand in inst_info.operands_mut() {
                    match operand {
                        Operand::Reg { reg, shift } => {
                            replace_reg(reg);
                            if let Some(shift) = shift {
                                match shift {
                                    Shift::Lsl(v) => replace_shift(v),
                                    Shift::Lsr(v) => replace_shift(v),
                                    Shift::Asr(v) => replace_shift(v),
                                    Shift::Ror(v) => replace_shift(v),
                                }
                            }
                        }
                        _ => {}
                    }
                }

                opcodes.push(inst_info.assemble());
            }

            BlockInst::Label { .. } | BlockInst::GuestPc(_) => {}
        }
    }
}

#[derive(Clone)]
pub struct GuestInstInfo(pub NonNull<InstInfo>);

impl GuestInstInfo {
    pub fn new(inst_info: &mut InstInfo) -> Self {
        GuestInstInfo(NonNull::from(inst_info))
    }
}

impl Debug for GuestInstInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

impl Deref for GuestInstInfo {
    type Target = InstInfo;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref() }
    }
}

impl DerefMut for GuestInstInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut() }
    }
}

#[derive(Copy, Clone)]
pub struct GuestPcInfo(pub u32);

impl Debug for GuestPcInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:x}", self.0)
    }
}

impl Deref for GuestPcInfo {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for GuestPcInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Debug)]
pub enum BlockInst {
    Alu3 {
        op: BlockAluOp,
        operands: [BlockOperandShift; 3],
        set_cond: BlockAluSetCond,
        thumb_pc_aligned: bool,
    },
    Alu2Op1 {
        op: BlockAluOp,
        operands: [BlockOperandShift; 2],
        set_cond: BlockAluSetCond,
        thumb_pc_aligned: bool,
    },
    Alu2Op0 {
        op: BlockAluOp,
        operands: [BlockOperandShift; 2],
        set_cond: BlockAluSetCond,
        thumb_pc_aligned: bool,
    },
    Transfer {
        op: BlockTransferOp,
        operands: [BlockOperandShift; 3],
        signed: bool,
        amount: MemoryAmount,
        add_to_base: bool,
    },
    TransferMultiple {
        op: BlockTransferOp,
        operand: BlockReg,
        regs: RegReserve,
        write_back: bool,
        pre: bool,
        add_to_base: bool,
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
    Mul {
        operands: [BlockOperandShift; 3],
        set_cond: BlockAluSetCond,
        thumb_pc_aligned: bool,
    },

    Label {
        label: BlockLabel,
        guest_pc: Option<GuestPcInfo>,
    },
    Branch {
        label: BlockLabel,
        cond: Cond,
        block_index: usize,
        skip: bool,
    },

    SaveContext {
        thread_regs_addr_reg: BlockReg,
        tmp_guest_cpsr_reg: BlockReg,
        regs_to_save: RegReserve,
    },
    SaveReg {
        guest_reg: Reg,
        reg_mapped: BlockReg,
        thread_regs_addr_reg: BlockReg,
        tmp_guest_cpsr_reg: BlockReg,
    },
    RestoreReg {
        guest_reg: Reg,
        reg_mapped: BlockReg,
        thread_regs_addr_reg: BlockReg,
        tmp_guest_cpsr_reg: BlockReg,
    },

    Call {
        func_reg: BlockReg,
        args: [Option<BlockReg>; 4],
    },
    Bkpt(u16),

    GuestPc(GuestPcInfo),
    GenericGuestInst {
        inst: GuestInstInfo,
        regs_mapping: [BlockReg; Reg::None as usize],
    },
}
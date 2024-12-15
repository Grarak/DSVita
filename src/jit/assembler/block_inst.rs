use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg, AluShiftImm, Bfc, Bfi, MulReg, Ubfx};
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, LdrStrImmSBHD, LdrStrReg, LdrStrRegSBHD, Mrs, Msr};
use crate::jit::assembler::arm::Bkpt;
use crate::jit::assembler::block_reg_allocator::ALLOCATION_REGS;
use crate::jit::assembler::block_reg_set::{block_reg_set, BlockRegSet};
use crate::jit::assembler::{BlockLabel, BlockOperand, BlockOperandShift, BlockReg, BlockShift};
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, ShiftType};
use bilge::prelude::*;
use std::cell::RefCell;
use std::fmt::{Debug, Formatter};
use std::hint::unreachable_unchecked;
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

#[derive(Clone, Eq, PartialEq)]
pub enum BlockAluSetCond {
    None,
    Host,
    HostGuest,
}

impl Debug for BlockAluSetCond {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockAluSetCond::None => write!(f, ""),
            BlockAluSetCond::Host => write!(f, "s"),
            BlockAluSetCond::HostGuest => write!(f, "s_guest"),
        }
    }
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

#[bitsize(32)]
#[derive(FromBits)]
pub struct BranchEncoding {
    pub index: u26,
    pub has_return: bool,
    pub is_call_common: bool,
    pub cond: u4,
}

#[derive(Clone)]
pub struct BlockInst {
    pub cond: Cond,
    pub kind: BlockInstKind,
    io_cache: RefCell<Option<(BlockRegSet, BlockRegSet)>>,
    pub skip: bool,
}

impl BlockInst {
    pub fn new(cond: Cond, kind: BlockInstKind) -> Self {
        BlockInst {
            cond,
            kind,
            io_cache: RefCell::new(None),
            skip: false,
        }
    }

    pub fn invalidate_io_cache(&self) {
        *self.io_cache.borrow_mut() = None;
    }

    pub fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        let mut cached_io = self.io_cache.borrow_mut();
        match *cached_io {
            None => {
                let (mut inputs, outputs) = self.kind.get_io();
                if self.cond != Cond::AL {
                    // For conditional insts initialize output guest regs as well
                    // Otherwise arbitrary values for regs will be saved
                    inputs.add_guests(outputs.get_guests());
                }
                *cached_io = Some((inputs, outputs));
                (inputs, outputs)
            }
            Some(cache) => cache,
        }
    }

    pub fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        *self.io_cache.borrow_mut() = None;
        self.kind.replace_input_regs(old, new);
    }

    pub fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        *self.io_cache.borrow_mut() = None;
        self.kind.replace_output_regs(old, new);
    }
}

impl From<BlockInstKind> for BlockInst {
    fn from(value: BlockInstKind) -> Self {
        BlockInst::new(Cond::AL, value)
    }
}

impl Debug for BlockInst {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.skip {
            write!(f, "SKIPPED: {:?} {:?}", self.cond, self.kind)
        } else {
            write!(f, "{:?} {:?}", self.cond, self.kind)
        }
    }
}

#[derive(Clone)]
pub enum BlockInstKind {
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
    GuestTransferMultiple {
        op: BlockTransferOp,
        addr_reg: BlockReg,
        addr_out_reg: BlockReg,
        gp_regs: RegReserve,
        fixed_regs: RegReserve,
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
    Bfi {
        operands: [BlockReg; 2],
        lsb: u8,
        width: u8,
    },
    Ubfx {
        operands: [BlockReg; 2],
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
        guest_pc: Option<u32>,
        unlikely: bool,
    },
    Branch {
        label: BlockLabel,
        block_index: usize,
        fallthrough: bool,
    },

    SaveContext {
        guest_regs: RegReserve,
        thread_regs_addr_reg: BlockReg,
    },
    SaveReg {
        guest_reg: Reg,
        reg_mapped: BlockReg,
        thread_regs_addr_reg: BlockReg,
    },
    RestoreReg {
        guest_reg: Reg,
        reg_mapped: BlockReg,
        thread_regs_addr_reg: BlockReg,
        tmp_guest_cpsr_reg: BlockReg,
    },
    MarkRegDirty {
        guest_reg: Reg,
        dirty: bool,
    },

    Call {
        func_reg: BlockReg,
        args: [Option<BlockReg>; 4],
        has_return: bool,
    },
    CallCommon {
        mem_offset: usize,
        args: [Option<BlockReg>; 4],
        has_return: bool,
    },
    Bkpt(u16),
    Nop,

    GuestPc(u32),
    GenericGuestInst {
        inst: GuestInstInfo,
        regs_mapping: [BlockReg; Reg::None as usize],
    },

    Prologue,
    Epilogue {
        restore_all_regs: bool,
    },

    PadBlock {
        label: BlockLabel,
        half: bool,
        correction: i32,
    },
}

impl BlockInstKind {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        match self {
            BlockInstKind::Alu3 { operands, set_cond, .. } | BlockInstKind::Mul { operands, set_cond, .. } => {
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
            BlockInstKind::Alu2Op1 { operands, set_cond, .. } => {
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
            BlockInstKind::Alu2Op0 { operands, set_cond, .. } => {
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
            BlockInstKind::Transfer { op, operands, .. } => match op {
                BlockTransferOp::Read => (
                    block_reg_set!(Some(operands[1].as_reg()), operands[2].try_as_reg(), operands[2].try_as_shift_reg()),
                    block_reg_set!(Some(operands[0].as_reg())),
                ),
                BlockTransferOp::Write => (
                    block_reg_set!(Some(operands[0].as_reg()), Some(operands[1].as_reg()), operands[2].try_as_reg(), operands[2].try_as_shift_reg()),
                    block_reg_set!(),
                ),
            },
            BlockInstKind::TransferMultiple { op, operand, regs, write_back, .. } => match op {
                BlockTransferOp::Read => (
                    block_reg_set!(Some(*operand)),
                    if *write_back { BlockRegSet::new_fixed(*regs) + *operand } else { BlockRegSet::new_fixed(*regs) },
                ),
                BlockTransferOp::Write => (BlockRegSet::new_fixed(*regs) + *operand, if *write_back { block_reg_set!(Some(*operand)) } else { block_reg_set!() }),
            },
            BlockInstKind::GuestTransferMultiple {
                op,
                addr_reg,
                addr_out_reg,
                gp_regs,
                fixed_regs,
                write_back,
                ..
            } => match op {
                BlockTransferOp::Read => {
                    let mut outputs = BlockRegSet::new_fixed(*fixed_regs);
                    outputs.add_guests(*gp_regs);
                    (block_reg_set!(Some(*addr_reg)), if *write_back { outputs + *addr_out_reg } else { outputs })
                }
                BlockTransferOp::Write => {
                    let mut inputs = BlockRegSet::new_fixed(*fixed_regs);
                    inputs.add_guests(*gp_regs);
                    (inputs + *addr_reg, if *write_back { block_reg_set!(Some(*addr_out_reg)) } else { block_reg_set!() })
                }
            },
            BlockInstKind::SystemReg { op, operand } => match op {
                BlockSystemRegOp::Mrs => (block_reg_set!(), block_reg_set!(Some(operand.as_reg()))),
                BlockSystemRegOp::Msr => (block_reg_set!(operand.try_as_reg()), block_reg_set!()),
            },
            BlockInstKind::Bfc { operand, .. } => (block_reg_set!(Some(*operand)), block_reg_set!(Some(*operand))),
            BlockInstKind::Bfi { operands, .. } | BlockInstKind::Ubfx { operands, .. } => (block_reg_set!(Some(operands[0]), Some(operands[1])), block_reg_set!(Some(operands[0]))),

            BlockInstKind::SaveContext { .. } => (block_reg_set!(), block_reg_set!()),
            BlockInstKind::SaveReg {
                guest_reg,
                reg_mapped,
                thread_regs_addr_reg,
            } => {
                let mut inputs = BlockRegSet::new();
                let mut outputs = BlockRegSet::new();
                match guest_reg {
                    Reg::CPSR => {
                        inputs += BlockReg::Fixed(Reg::CPSR);
                        inputs += *thread_regs_addr_reg;
                        outputs += *reg_mapped;
                    }
                    _ => {
                        inputs += *reg_mapped;
                        inputs += *thread_regs_addr_reg;
                    }
                }
                (inputs, outputs)
            }
            BlockInstKind::RestoreReg {
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
            BlockInstKind::MarkRegDirty { guest_reg, dirty: true } => (block_reg_set!(Some(BlockReg::from(*guest_reg))), block_reg_set!(Some(BlockReg::from(*guest_reg)))),
            BlockInstKind::MarkRegDirty { guest_reg, dirty: false } => (block_reg_set!(Some(BlockReg::from(*guest_reg))), block_reg_set!()),

            BlockInstKind::Call { func_reg, args, has_return } => {
                let mut inputs = BlockRegSet::new();
                inputs += *func_reg;
                for &arg in args.iter().flatten() {
                    inputs += arg;
                }
                (
                    inputs,
                    block_reg_set!(
                        Some(BlockReg::Fixed(Reg::R0)),
                        Some(BlockReg::Fixed(Reg::R1)),
                        Some(BlockReg::Fixed(Reg::R2)),
                        Some(BlockReg::Fixed(Reg::R3)),
                        Some(BlockReg::Fixed(Reg::R12)),
                        Some(BlockReg::Fixed(Reg::CPSR)),
                        if *has_return { Some(BlockReg::Fixed(Reg::LR)) } else { None }
                    ),
                )
            }
            BlockInstKind::CallCommon { args, has_return, .. } => {
                let mut inputs = BlockRegSet::new();
                for &arg in args.iter().flatten() {
                    inputs += arg;
                }
                (
                    inputs,
                    block_reg_set!(
                        Some(BlockReg::Fixed(Reg::R0)),
                        Some(BlockReg::Fixed(Reg::R1)),
                        Some(BlockReg::Fixed(Reg::R2)),
                        Some(BlockReg::Fixed(Reg::R3)),
                        Some(BlockReg::Fixed(Reg::R12)),
                        Some(BlockReg::Fixed(Reg::CPSR)),
                        if *has_return { Some(BlockReg::Fixed(Reg::LR)) } else { None }
                    ),
                )
            }
            BlockInstKind::GenericGuestInst { inst, regs_mapping } => {
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

            BlockInstKind::Prologue => (
                block_reg_set!(Some(BlockReg::Fixed(Reg::SP)), Some(BlockReg::Fixed(Reg::LR))),
                block_reg_set!(Some(BlockReg::Fixed(Reg::SP))),
            ),
            BlockInstKind::Epilogue { .. } => (
                block_reg_set!(Some(BlockReg::Fixed(Reg::SP))),
                block_reg_set!(Some(BlockReg::Fixed(Reg::SP)), Some(BlockReg::Fixed(Reg::PC))),
            ),

            BlockInstKind::Label { .. } | BlockInstKind::Branch { .. } | BlockInstKind::GuestPc(_) | BlockInstKind::Bkpt(_) | BlockInstKind::Nop | BlockInstKind::PadBlock { .. } => {
                (block_reg_set!(), block_reg_set!())
            }
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

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        match self {
            BlockInstKind::Alu3 { operands, .. } | BlockInstKind::Mul { operands, .. } => {
                operands[1].replace_regs(old, new);
                operands[2].replace_regs(old, new);
            }
            BlockInstKind::Alu2Op1 { operands, .. } => Self::replace_shift_operands(operands, old, new),
            BlockInstKind::Alu2Op0 { operands, .. } => operands[1].replace_regs(old, new),
            BlockInstKind::Transfer { op, operands, .. } => {
                if *op == BlockTransferOp::Write {
                    operands[0].replace_regs(old, new);
                }
                operands[1].replace_regs(old, new);
                operands[2].replace_regs(old, new);
            }
            BlockInstKind::TransferMultiple { operand, .. } => Self::replace_reg(operand, old, new),
            BlockInstKind::GuestTransferMultiple { addr_reg, .. } => Self::replace_reg(addr_reg, old, new),
            BlockInstKind::SystemReg { op, operand } => {
                if *op == BlockSystemRegOp::Msr {
                    Self::replace_operand(operand, old, new);
                }
            }
            BlockInstKind::Bfc { operand, .. } => Self::replace_reg(operand, old, new),
            BlockInstKind::Bfi { operands, .. } => {
                Self::replace_reg(&mut operands[0], old, new);
                Self::replace_reg(&mut operands[1], old, new);
            }
            BlockInstKind::Ubfx { operands, .. } => Self::replace_reg(&mut operands[1], old, new),
            BlockInstKind::SaveContext { .. } => {
                unreachable!()
            }
            BlockInstKind::SaveReg {
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
            BlockInstKind::RestoreReg { thread_regs_addr_reg, .. } => Self::replace_reg(thread_regs_addr_reg, old, new),
            BlockInstKind::Call { func_reg, .. } => Self::replace_reg(func_reg, old, new),
            BlockInstKind::GenericGuestInst { inst, regs_mapping } => {
                for reg in inst.src_regs {
                    Self::replace_reg(&mut regs_mapping[reg as usize], old, new);
                }
            }
            BlockInstKind::CallCommon { .. }
            | BlockInstKind::Label { .. }
            | BlockInstKind::Branch { .. }
            | BlockInstKind::MarkRegDirty { .. }
            | BlockInstKind::GuestPc(_)
            | BlockInstKind::Bkpt(_)
            | BlockInstKind::Nop
            | BlockInstKind::Prologue
            | BlockInstKind::Epilogue { .. }
            | BlockInstKind::PadBlock { .. } => {}
        }
    }

    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        match self {
            BlockInstKind::Alu3 { operands, .. } | BlockInstKind::Mul { operands, .. } => operands[0].replace_regs(old, new),
            BlockInstKind::Alu2Op1 { .. } => {}
            BlockInstKind::Alu2Op0 { operands, .. } => operands[0].replace_regs(old, new),
            BlockInstKind::Transfer { op, operands, .. } => {
                if *op == BlockTransferOp::Read {
                    operands[0].replace_regs(old, new);
                }
            }
            BlockInstKind::TransferMultiple { operand, write_back, .. } => {
                if *write_back {
                    Self::replace_reg(operand, old, new);
                }
            }
            BlockInstKind::GuestTransferMultiple { addr_out_reg, write_back, .. } => {
                if *write_back {
                    Self::replace_reg(addr_out_reg, old, new);
                }
            }
            BlockInstKind::SystemReg { op, operand } => {
                if *op == BlockSystemRegOp::Mrs {
                    Self::replace_operand(operand, old, new);
                }
            }
            BlockInstKind::Bfc { operand, .. } => Self::replace_reg(operand, old, new),
            BlockInstKind::Bfi { operands, .. } | BlockInstKind::Ubfx { operands, .. } => Self::replace_reg(&mut operands[0], old, new),
            BlockInstKind::SaveContext { .. } => {}
            BlockInstKind::SaveReg { guest_reg, reg_mapped, .. } => {
                if *guest_reg == Reg::CPSR {
                    Self::replace_reg(reg_mapped, old, new);
                }
            }
            BlockInstKind::RestoreReg {
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
            BlockInstKind::Call { .. } => {}
            BlockInstKind::GenericGuestInst { inst, regs_mapping } => {
                for reg in inst.out_regs {
                    Self::replace_reg(&mut regs_mapping[reg as usize], old, new);
                }
            }
            BlockInstKind::CallCommon { .. }
            | BlockInstKind::Label { .. }
            | BlockInstKind::Branch { .. }
            | BlockInstKind::MarkRegDirty { .. }
            | BlockInstKind::GuestPc(_)
            | BlockInstKind::Bkpt(_)
            | BlockInstKind::Nop
            | BlockInstKind::Prologue
            | BlockInstKind::Epilogue { .. }
            | BlockInstKind::PadBlock { .. } => {}
        }
    }

    fn save_guest_cpsr(opcodes: &mut Vec<u32>, thread_regs_addr_reg: Reg, host_reg: Reg) {
        opcodes.push(Mrs::cpsr(host_reg, Cond::AL));
        // Only copy the cond flags from host cpsr
        opcodes.push(AluShiftImm::mov(host_reg, host_reg, ShiftType::Lsr, 24, Cond::AL));
        opcodes.push(LdrStrImm::strb_offset_al(host_reg, thread_regs_addr_reg, Reg::CPSR as u16 * 4 + 3));
    }

    pub fn emit_opcode(&mut self, opcodes: &mut Vec<u32>, opcode_index: usize, branch_placeholders: &mut Vec<usize>, used_host_regs: RegReserve) {
        let alu_reg = |op: BlockAluOp, op0: BlockReg, op1: BlockReg, op2: BlockReg, shift: BlockShift, set_cond: bool| match shift.value {
            BlockOperand::Reg(shift_reg) => AluReg::generic(op as u8, op0.as_fixed(), op1.as_fixed(), op2.as_fixed(), shift.shift_type, shift_reg.as_fixed(), set_cond, Cond::AL),
            BlockOperand::Imm(shift_imm) => {
                debug_assert_eq!(shift_imm & !0x1F, 0);
                AluShiftImm::generic(op as u8, op0.as_fixed(), op1.as_fixed(), op2.as_fixed(), shift.shift_type, shift_imm as u8, set_cond, Cond::AL)
            }
        };
        let alu_imm = |op: BlockAluOp, op0: BlockReg, op1: BlockReg, op2: u32, shift: BlockShift, set_cond: bool| {
            debug_assert_eq!(op2 & !0xFF, 0);
            let shift_value = shift.value.as_imm();
            debug_assert_eq!(shift_value & !0xF, 0);
            debug_assert!(shift_value == 0 || shift.shift_type == ShiftType::Ror);
            AluImm::generic(op as u8, op0.as_fixed(), op1.as_fixed(), op2 as u8, shift_value as u8, set_cond, Cond::AL)
        };

        match self {
            BlockInstKind::Alu3 { op, operands, set_cond, .. } => match operands[2].operand {
                BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, operands[0].as_reg(), operands[1].as_reg(), reg, operands[2].shift, *set_cond != BlockAluSetCond::None)),
                BlockOperand::Imm(imm) => opcodes.push(alu_imm(*op, operands[0].as_reg(), operands[1].as_reg(), imm, operands[2].shift, *set_cond != BlockAluSetCond::None)),
            },
            BlockInstKind::Alu2Op1 { op, operands, set_cond, .. } => {
                debug_assert_ne!(*set_cond, BlockAluSetCond::None);
                match operands[1].operand {
                    BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, BlockReg::Fixed(Reg::R0), operands[0].as_reg(), reg, operands[1].shift, true)),
                    BlockOperand::Imm(imm) => opcodes.push(alu_imm(*op, BlockReg::Fixed(Reg::R0), operands[0].as_reg(), imm, operands[1].shift, true)),
                }
            }
            BlockInstKind::Alu2Op0 { op, operands, set_cond, .. } => match operands[1].operand {
                BlockOperand::Reg(reg) => {
                    if *op == BlockAluOp::Mov && operands[0].as_reg() == reg && operands[1].shift == BlockShift::default() && *set_cond == BlockAluSetCond::None {
                        return;
                    }
                    opcodes.push(alu_reg(*op, operands[0].as_reg(), BlockReg::Fixed(Reg::R0), reg, operands[1].shift, *set_cond != BlockAluSetCond::None))
                }
                BlockOperand::Imm(imm) => {
                    if *op == BlockAluOp::Mov && operands[1].shift == BlockShift::default() && *set_cond == BlockAluSetCond::None {
                        opcodes.extend(AluImm::mov32(operands[0].as_reg().as_fixed(), imm))
                    } else {
                        opcodes.push(alu_imm(*op, operands[0].as_reg(), BlockReg::Fixed(Reg::R0), imm, operands[1].shift, *set_cond != BlockAluSetCond::None))
                    }
                }
            },
            BlockInstKind::Transfer {
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
                                debug_assert_eq!(shift_amount, 0);
                                LdrStrRegSBHD::generic(op0, op1, op2, true, MemoryAmount::Byte, read, false, add_to_base, true, cond)
                            } else {
                                LdrStrReg::generic(op0, op1, op2, shift_amount, shift_type, read, false, true, add_to_base, true, cond)
                            }
                        },
                        MemoryAmount::Half => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, _: ShiftType, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            debug_assert_eq!(shift_amount, 0);
                            LdrStrRegSBHD::generic(op0, op1, op2, signed, MemoryAmount::Half, read, false, add_to_base, true, cond)
                        },
                        MemoryAmount::Word => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            debug_assert!(!signed);
                            LdrStrReg::generic(op0, op1, op2, shift_amount, shift_type, read, false, false, add_to_base, true, cond)
                        },
                        MemoryAmount::Double => {
                            todo!()
                        }
                    };
                    let shift = operands[2].as_shift_imm();
                    debug_assert_eq!(shift & !0x1F, 0);
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
                                debug_assert_eq!(imm_offset & !0xFF, 0);
                                LdrStrImmSBHD::generic(op0, op1, imm_offset as u8, true, MemoryAmount::Byte, read, false, true, true, cond)
                            } else {
                                LdrStrImm::generic(op0, op1, imm_offset, read, false, true, add_to_base, true, cond)
                            }
                        },
                        MemoryAmount::Half => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            debug_assert_eq!(imm_offset & !0xFF, 0);
                            LdrStrImmSBHD::generic(op0, op1, imm_offset as u8, signed, MemoryAmount::Half, read, false, add_to_base, true, cond)
                        },
                        MemoryAmount::Word => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                            debug_assert!(!signed);
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
            BlockInstKind::TransferMultiple {
                op,
                operand,
                regs,
                write_back,
                pre,
                add_to_base,
            } => opcodes.push(LdmStm::generic(operand.as_fixed(), *regs, *op == BlockTransferOp::Read, *write_back, *add_to_base, *pre, Cond::AL)),
            BlockInstKind::GuestTransferMultiple {
                op,
                addr_reg,
                addr_out_reg,
                gp_regs,
                fixed_regs,
                write_back,
                pre,
                add_to_base,
            } => {
                if *write_back && *addr_reg != *addr_out_reg {
                    opcodes.push(AluShiftImm::mov_al(addr_out_reg.as_fixed(), addr_reg.as_fixed()))
                }
                opcodes.push(LdmStm::generic(
                    if *write_back { addr_out_reg.as_fixed() } else { addr_reg.as_fixed() },
                    *gp_regs + *fixed_regs,
                    *op == BlockTransferOp::Read,
                    *write_back,
                    *add_to_base,
                    *pre,
                    Cond::AL,
                ))
            }
            BlockInstKind::SystemReg { op, operand } => match op {
                BlockSystemRegOp::Mrs => opcodes.push(Mrs::cpsr(operand.as_reg().as_fixed(), Cond::AL)),
                BlockSystemRegOp::Msr => opcodes.push(Msr::cpsr_flags(operand.as_reg().as_fixed(), Cond::AL)),
            },
            BlockInstKind::Bfc { operand, lsb, width } => opcodes.push(Bfc::create(operand.as_fixed(), *lsb, *width, Cond::AL)),
            BlockInstKind::Bfi { operands, lsb, width } => opcodes.push(Bfi::create(operands[0].as_fixed(), operands[1].as_fixed(), *lsb, *width, Cond::AL)),
            BlockInstKind::Ubfx { operands, lsb, width } => opcodes.push(Ubfx::create(operands[0].as_fixed(), operands[1].as_fixed(), *lsb, *width, Cond::AL)),
            BlockInstKind::Mul { operands, set_cond, .. } => match operands[2].operand {
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

            BlockInstKind::Branch { block_index, .. } => {
                // Encode label
                // Branch offset can only be figured out later
                opcodes.push(BranchEncoding::new(u26::new(*block_index as u32), false, false, u4::new(Cond::AL as u8)).into());
                branch_placeholders.push(opcode_index);
            }

            BlockInstKind::SaveContext { .. } => unsafe { unreachable_unchecked() },
            BlockInstKind::SaveReg {
                guest_reg,
                reg_mapped,
                thread_regs_addr_reg,
                ..
            } => match guest_reg {
                Reg::CPSR => Self::save_guest_cpsr(opcodes, thread_regs_addr_reg.as_fixed(), reg_mapped.as_fixed()),
                _ => opcodes.push(LdrStrImm::str_offset_al(reg_mapped.as_fixed(), thread_regs_addr_reg.as_fixed(), *guest_reg as u16 * 4)),
            },
            BlockInstKind::RestoreReg {
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

            BlockInstKind::Call { func_reg, has_return, .. } => opcodes.push(if *has_return {
                Bx::blx(func_reg.as_fixed(), Cond::AL)
            } else {
                Bx::bx(func_reg.as_fixed(), Cond::AL)
            }),
            BlockInstKind::CallCommon { mem_offset, has_return, .. } => {
                // Encode common offset
                // Branch offset can only be figured out later
                opcodes.push(BranchEncoding::new(u26::new(*mem_offset as u32), *has_return, true, u4::new(Cond::AL as u8)).into());
                branch_placeholders.push(opcode_index);
            }
            BlockInstKind::Bkpt(id) => opcodes.push(Bkpt::bkpt(*id)),
            BlockInstKind::Nop => opcodes.push(AluShiftImm::mov_al(Reg::R0, Reg::R0)),

            BlockInstKind::GenericGuestInst { inst, regs_mapping } => {
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
                    if let Operand::Reg { reg, shift } = operand {
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
                }

                inst_info.set_cond(Cond::AL);
                opcodes.push(inst_info.assemble());
            }

            BlockInstKind::Prologue => opcodes.push(LdmStm::generic(Reg::SP, used_host_regs + Reg::LR, false, true, false, true, Cond::AL)),
            BlockInstKind::Epilogue { restore_all_regs } => opcodes.push(LdmStm::generic(
                Reg::SP,
                if *restore_all_regs { ALLOCATION_REGS + Reg::R12 } else { used_host_regs } + Reg::PC,
                true,
                true,
                true,
                false,
                Cond::AL,
            )),

            BlockInstKind::Label { .. } | BlockInstKind::MarkRegDirty { .. } | BlockInstKind::GuestPc(_) | BlockInstKind::PadBlock { .. } => {}
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

impl Debug for BlockInstKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let write_alu = |op, operands: &[BlockOperandShift], set_cond, thumb_pc_aligned, f: &mut Formatter<'_>| write!(f, "{op:?}{set_cond:?} {operands:?}, align pc: {thumb_pc_aligned}");
        match self {
            BlockInstKind::Alu3 {
                op,
                operands,
                set_cond,
                thumb_pc_aligned,
            } => write_alu(op, operands, set_cond, thumb_pc_aligned, f),
            BlockInstKind::Alu2Op1 {
                op,
                operands,
                set_cond,
                thumb_pc_aligned,
            } => write_alu(op, operands, set_cond, thumb_pc_aligned, f),
            BlockInstKind::Alu2Op0 {
                op,
                operands,
                set_cond,
                thumb_pc_aligned,
            } => write_alu(op, operands, set_cond, thumb_pc_aligned, f),
            BlockInstKind::Transfer {
                op,
                operands,
                signed,
                amount,
                add_to_base,
            } => {
                let signed = if *signed { "S" } else { "U" };
                let amount = match amount {
                    MemoryAmount::Byte => "8",
                    MemoryAmount::Half => "16",
                    MemoryAmount::Word => "32",
                    MemoryAmount::Double => "64",
                };
                let add_to_base = if *add_to_base { "+" } else { "-" };
                write!(f, "{op:?}{signed}{amount} {:?} [{:?}, {:?}], {add_to_base}base", operands[0], operands[1], operands[2])
            }
            BlockInstKind::TransferMultiple {
                op,
                operand,
                regs,
                write_back,
                pre,
                add_to_base,
            } => {
                let add_to_base = if *add_to_base { "+" } else { "-" };
                write!(f, "{op:?}M {operand:?} {regs:?}, write back: {write_back}, pre {pre}, {add_to_base}base")
            }
            BlockInstKind::GuestTransferMultiple {
                op,
                addr_reg,
                addr_out_reg,
                gp_regs,
                fixed_regs,
                write_back,
                pre,
                add_to_base,
            } => {
                let add_to_base = if *add_to_base { "+" } else { "-" };
                write!(
                    f,
                    "Guest{op:?}M {addr_reg:?} -> {addr_out_reg:?} gp regs: {gp_regs:?}, fixed regs: {fixed_regs:?}, write back: {write_back}, pre {pre}, {add_to_base}base"
                )
            }
            BlockInstKind::SystemReg { op, operand } => write!(f, "{op:?} {operand:?}"),
            BlockInstKind::Bfc { operand, lsb, width } => write!(f, "Bfc {operand:?}, {lsb}, {width}"),
            BlockInstKind::Bfi { operands, lsb, width } => write!(f, "Bfi {:?}, {:?}, {lsb}, {width}", operands[0], operands[1]),
            BlockInstKind::Ubfx { operands, lsb, width } => write!(f, "Ubfx {:?}, {:?}, {lsb}, {width}", operands[0], operands[1]),
            BlockInstKind::Mul { operands, set_cond, thumb_pc_aligned } => write!(f, "Mul{set_cond:?} {operands:?}, align pc: {thumb_pc_aligned}"),
            BlockInstKind::Label { label, guest_pc, unlikely } => {
                let guest_pc = match guest_pc {
                    None => "",
                    Some(pc) => &format!(" {pc:x}"),
                };
                write!(f, "Label {label:?}{guest_pc} unlikely: {unlikely}")
            }
            BlockInstKind::Branch { label, block_index, .. } => write!(f, "B {label:?}, block index: {block_index}"),
            BlockInstKind::SaveContext { .. } => write!(f, "SaveContext"),
            BlockInstKind::SaveReg { guest_reg, reg_mapped, .. } => write!(f, "SaveReg {guest_reg:?}, mapped: {reg_mapped:?}"),
            BlockInstKind::RestoreReg { guest_reg, reg_mapped, .. } => write!(f, "RestoreReg {guest_reg:?}, mapped: {reg_mapped:?}"),
            BlockInstKind::MarkRegDirty { guest_reg, dirty } => {
                if *dirty {
                    write!(f, "Dirty {guest_reg:?}")
                } else {
                    write!(f, "Undirty {guest_reg:?}")
                }
            }
            BlockInstKind::Call { func_reg, args, has_return } => {
                if *has_return {
                    write!(f, "Blx {func_reg:?} {args:?}")
                } else {
                    write!(f, "Bx {func_reg:?} {args:?}")
                }
            }
            BlockInstKind::CallCommon { mem_offset, args, has_return } => {
                if *has_return {
                    write!(f, "Bl {mem_offset:x} {args:?}")
                } else {
                    write!(f, "B {mem_offset:x} {args:?}")
                }
            }
            BlockInstKind::Bkpt(id) => write!(f, "Bkpt {id}"),
            BlockInstKind::Nop => write!(f, "Nop"),
            BlockInstKind::GuestPc(pc) => write!(f, "GuestPc {pc:x}"),
            BlockInstKind::GenericGuestInst { inst, .. } => write!(f, "{inst:?}"),
            BlockInstKind::Prologue => write!(f, "Prologue"),
            BlockInstKind::Epilogue { restore_all_regs } => write!(f, "Epilogue restore all regs {restore_all_regs}"),
            BlockInstKind::PadBlock { label, half, correction } => write!(f, "PadBlock {label:?} half: {half} {correction}"),
        }
    }
}

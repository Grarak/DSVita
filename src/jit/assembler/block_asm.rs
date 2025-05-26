use crate::jit::assembler::reg_alloc::{RegAlloc, GUEST_REGS_LENGTH};
use crate::jit::assembler::vixl::vixl::{
    BranchHint_kNear, FlagsUpdate_DontCare, InstructionSet_A32, InstructionSet_T32, MaskedSpecialRegisterType_CPSR_f, MemOperand, ShiftType_ASR, ShiftType_LSL, ShiftType_LSR, ShiftType_ROR,
    ShiftType_RRX, SpecialRegisterType_CPSR,
};
use crate::jit::assembler::vixl::{
    vixl, Label, MacroAssembler, MasmAdd5, MasmB2, MasmBlx1, MasmLdr2, MasmLsr5, MasmMov4, MasmMrs2, MasmMsr2, MasmPop1, MasmPush1, MasmStr2, MasmStrb2, MasmStrd3, MasmSub5,
};
use crate::jit::inst_info::{InstInfo, Operands, Shift, ShiftValue};
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::{inst_info, Cond};
use crate::mmap::{PAGE_SHIFT, PAGE_SIZE};
use std::ops::{Deref, DerefMut};

pub const GUEST_REGS_PTR_REG: Reg = Reg::R12; // Also used for function calls
pub const GUEST_REGS_PTR_STACK_OFFSET: u32 = 0;
pub const MMU_OFFSET_STACK_OFFSET: u32 = 4;
pub const CPSR_TMP_REG: Reg = Reg::R0;

#[derive(Clone)]
pub struct GuestInstMetadata {
    pub opcode_offset: usize,
    pub pc: u32,
    pub total_cycle_count: u16,
    pub op: Op,
    pub operands: Operands,
    pub op0: Reg,
    pub dirty_guest_regs: RegReserve,
    pub mapped_guest_regs: [Reg; GUEST_REGS_LENGTH],
}

impl GuestInstMetadata {
    pub fn new(opcode_offset: usize, pc: u32, total_cycle_count: u16, op: Op, operands: Operands, op0: Reg, dirty_guest_regs: RegReserve, mapped_guest_regs: [Reg; GUEST_REGS_LENGTH]) -> Self {
        GuestInstMetadata {
            opcode_offset,
            pc,
            total_cycle_count,
            op,
            operands,
            op0,
            dirty_guest_regs,
            mapped_guest_regs,
        }
    }
}

pub struct BlockAsm {
    masm: MacroAssembler,
    reg_alloc: RegAlloc,
    pub current_pc: u32,
    pub thumb: bool,
    pub dirty_guest_regs: RegReserve,
    pub guest_inst_metadata: Vec<(u16, GuestInstMetadata)>,
    pub guest_basic_block_labels: Vec<Option<Label>>,
}

impl BlockAsm {
    pub fn new(thumb: bool) -> Self {
        BlockAsm {
            masm: MacroAssembler::new(if thumb { InstructionSet_T32 } else { InstructionSet_A32 }),
            reg_alloc: RegAlloc::new(),
            current_pc: 0,
            thumb,
            dirty_guest_regs: RegReserve::new(),
            guest_inst_metadata: Vec::new(),
            guest_basic_block_labels: Vec::new(),
        }
    }

    pub fn prologue(&mut self, guest_regs_ptr: *mut u32, mmu_offset: *mut u8, basic_block_len: usize) {
        self.push1(reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::LR));
        self.sub5(FlagsUpdate_DontCare, Cond::AL, Reg::SP, Reg::SP, &(3 * 4).into());
        self.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &(guest_regs_ptr as u32).into());
        self.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &(mmu_offset as u32).into());
        self.strd3(Reg::R0, Reg::R1, &MemOperand::reg_offset(Reg::SP, GUEST_REGS_PTR_STACK_OFFSET as i32));

        self.guest_basic_block_labels.resize_with(basic_block_len, || None);
    }

    pub fn restore_stack(&mut self) {
        self.add5(FlagsUpdate_DontCare, Cond::AL, Reg::SP, Reg::SP, &(3 * 4).into());
        self.pop1(reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::LR));
    }

    pub fn exit_guest_context(&mut self, host_sp_ptr: *mut usize) {
        self.ldr2(Reg::R0, host_sp_ptr as u32);
        self.ldr2(Reg::SP, &MemOperand::reg(Reg::R0));
        self.pop1(reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::R12, Reg::PC));
    }

    pub fn epilogue(&mut self) {
        self.add5(FlagsUpdate_DontCare, Cond::AL, Reg::SP, Reg::SP, &(3 * 4).into());
        self.pop1(reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::PC));
    }

    pub fn init_guest_regs(&mut self, guest_regs: RegReserve) {
        self.reg_alloc = RegAlloc::new();

        self.ldr2(GUEST_REGS_PTR_REG, &MemOperand::reg_offset(Reg::SP, GUEST_REGS_PTR_STACK_OFFSET as i32));

        if guest_regs.is_reserved(Reg::CPSR) {
            self.load_guest_reg(CPSR_TMP_REG, Reg::CPSR);
            self.msr2(MaskedSpecialRegisterType_CPSR_f.into(), &CPSR_TMP_REG.into());
        }

        self.dirty_guest_regs.clear();
    }

    pub fn call(&mut self, fun: *const ()) {
        self.ldr2(Reg::R12, fun as u32);
        self.blx1(Reg::R12);
    }

    pub fn alloc_guest_inst(&mut self, inst: &InstInfo, next_live_regs: RegReserve) {
        let mut input_regs = inst.src_regs;
        let output_regs = inst.out_regs;
        if inst.cond != Cond::AL {
            input_regs += output_regs;
        }

        if inst.op.is_single_mem_transfer() && !inst.op.is_write_mem_transfer() {
            let op1 = inst.operands()[1].as_reg_no_shift().unwrap();
            let op2 = inst.operands()[2].as_imm();

            let transfer = match inst.op {
                Op::Ldr(transfer) | Op::LdrT(transfer) => transfer,
                _ => unreachable!(),
            };

            if op1 == Reg::PC && op2.is_some() && !transfer.write_back() {
                self.alloc_guest_regs(input_regs - Reg::PC, output_regs, inst.cond, next_live_regs);
                return;
            }
        }

        self.alloc_guest_regs(input_regs, output_regs, inst.cond, next_live_regs);

        if inst.src_regs.is_reserved(Reg::PC) {
            let pc_reg = self.reg_alloc.get_guest_map(Reg::PC);
            let pc = if self.thumb {
                let mut pc = self.current_pc + 4;
                if inst.op.is_alu() && !inst.op.is_thumb_alu_high() {
                    pc &= !0x3;
                }
                pc
            } else {
                let mut pc = self.current_pc + 8;
                if inst.op.is_alu() {
                    if let Some(inst_info::Operand::Reg { reg: op2_reg, shift: Some(shift) }) = inst.operands().last() {
                        if let ShiftValue::Reg(_) = match shift {
                            Shift::Lsl(value) => value,
                            Shift::Lsr(value) => value,
                            Shift::Asr(value) => value,
                            Shift::Ror(value) => value,
                        } {
                            if *op2_reg == Reg::PC || (inst.operands().len() == 3 && inst.operands()[1].as_reg_no_shift().unwrap() == Reg::PC) {
                                pc += 4;
                            }
                        }
                    }
                }
                pc
            };

            self.ldr2(pc_reg, pc);
        }
    }

    pub fn alloc_guest_regs(&mut self, input_regs: RegReserve, output_regs: RegReserve, cond: Cond, next_live_regs: RegReserve) {
        let spilled_guest_regs = self.reg_alloc.alloc_guest_regs(input_regs - Reg::CPSR, output_regs - Reg::CPSR, next_live_regs, &mut self.masm);
        if cond == Cond::AL {
            self.dirty_guest_regs -= spilled_guest_regs;
        }
    }

    pub fn load_mmu_offset(&mut self, dest_reg: Reg) {
        self.ldr2(dest_reg, &MemOperand::reg_offset(Reg::SP, MMU_OFFSET_STACK_OFFSET as i32));
    }

    pub fn load_guest_reg(&mut self, dest_reg: Reg, guest_reg: Reg) {
        self.ldr2(dest_reg, &MemOperand::reg_offset(GUEST_REGS_PTR_REG, guest_reg as i32 * 4));
    }

    pub fn store_guest_reg(&mut self, src_reg: Reg, guest_reg: Reg) {
        self.str2(src_reg, &MemOperand::reg_offset(GUEST_REGS_PTR_REG, guest_reg as i32 * 4));
    }

    pub fn save_dirty_guest_cpsr(&mut self, clear: bool) {
        if self.dirty_guest_regs.is_reserved(Reg::CPSR) {
            self.mrs2(CPSR_TMP_REG, SpecialRegisterType_CPSR.into());
            self.lsr5(FlagsUpdate_DontCare, Cond::AL, CPSR_TMP_REG, CPSR_TMP_REG, &24.into());
            self.strb2(CPSR_TMP_REG, &MemOperand::reg_offset(GUEST_REGS_PTR_REG, Reg::CPSR as i32 * 4 + 3));
        }
        if clear {
            self.dirty_guest_regs -= Reg::CPSR;
        }
    }

    pub fn save_dirty_guest_regs(&mut self, cpsr: bool, clear: bool) {
        self.save_dirty_guest_regs_additional(cpsr, clear, RegReserve::new());
    }

    pub fn save_dirty_guest_regs_additional(&mut self, cpsr: bool, clear: bool, additional_guest_regs: RegReserve) {
        self.reg_alloc.save_active_guest_regs(self.dirty_guest_regs - Reg::CPSR + additional_guest_regs, &mut self.masm);
        if cpsr {
            self.save_dirty_guest_cpsr(clear);
        }
        if clear {
            self.dirty_guest_regs.clear();
        }
    }

    pub fn get_dirty_guest_regs(&self) -> RegReserve {
        self.dirty_guest_regs
    }

    pub fn add_dirty_guest_regs(&mut self, guest_regs: RegReserve) {
        self.dirty_guest_regs += guest_regs;
    }

    pub fn get_guest_map(&mut self, guest_reg: Reg) -> Reg {
        self.reg_alloc.get_guest_map(guest_reg)
    }

    pub fn get_free_host_regs(&self) -> RegReserve {
        self.reg_alloc.free_regs
    }

    pub fn get_guest_operand_map(&mut self, guest_operand: &inst_info::Operand) -> vixl::Operand {
        match guest_operand {
            inst_info::Operand::Reg { reg, shift } => {
                let map_reg = self.reg_alloc.get_guest_map(*reg).into();
                match shift {
                    None => unsafe { vixl::Operand::new2(map_reg) },
                    Some(shift) => {
                        let (shift_type, value) = match shift {
                            Shift::Lsl(value) => (ShiftType_LSL, value),
                            Shift::Lsr(value) => (ShiftType_LSR, value),
                            Shift::Asr(value) => (ShiftType_ASR, value),
                            Shift::Ror(value) => (ShiftType_ROR, value),
                        };
                        match value {
                            ShiftValue::Reg(shift_reg) => unsafe { vixl::Operand::new5(map_reg, shift_type.into(), self.reg_alloc.get_guest_map(*shift_reg).into()) },
                            ShiftValue::Imm(shift_imm) => {
                                let mut shift_imm = *shift_imm;
                                if shift_imm == 0 {
                                    match shift_type {
                                        ShiftType_LSR | ShiftType_ASR => shift_imm = 32,
                                        ShiftType_ROR => return unsafe { vixl::Operand::new3(map_reg, ShiftType_RRX.into()) },
                                        _ => {}
                                    }
                                }
                                unsafe { vixl::Operand::new4(map_reg, shift_type.into(), shift_imm as u32) }
                            }
                        }
                    }
                }
            }
            inst_info::Operand::Imm(imm) => unsafe { vixl::Operand::new(*imm) },
            _ => unreachable!(),
        }
    }

    pub fn restore_guest_regs_ptr(&mut self) {
        self.ldr2(GUEST_REGS_PTR_REG, &MemOperand::reg_offset(Reg::SP, GUEST_REGS_PTR_STACK_OFFSET as i32));
    }

    pub fn restore_tmp_regs(&mut self, next_live_regs: RegReserve) {
        self.restore_guest_regs_ptr();
        if next_live_regs.is_reserved(Reg::CPSR) {
            self.load_guest_reg(CPSR_TMP_REG, Reg::CPSR);
            self.msr2(MaskedSpecialRegisterType_CPSR_f.into(), &CPSR_TMP_REG.into());
        }
    }

    pub fn reload_active_guest_regs(&mut self, guest_regs: RegReserve) {
        self.reg_alloc.reload_active_guest_regs(guest_regs, &mut self.masm);
    }

    pub fn reload_active_guest_regs_all(&mut self) {
        self.reg_alloc.reload_active_guest_regs(RegReserve::all(), &mut self.masm);
    }

    pub fn guest_inst_metadata(&mut self, total_cycles_reg: u16, inst: &InstInfo, op0: Reg, mut dirty_guest_regs: RegReserve) {
        let offset = self.get_cursor_offset();
        let page_num = (offset >> PAGE_SHIFT) as u16;
        let mut pc = self.current_pc;
        if self.thumb {
            pc |= 1;
        }
        for guest_reg in dirty_guest_regs - Reg::CPSR {
            if self.get_guest_map(guest_reg) == Reg::None {
                dirty_guest_regs -= guest_reg;
            }
        }
        self.guest_inst_metadata.push((
            page_num,
            GuestInstMetadata::new(
                offset as usize & (PAGE_SIZE - 1),
                pc,
                total_cycles_reg,
                inst.op,
                inst.operands,
                op0,
                dirty_guest_regs,
                self.reg_alloc.guest_regs_mapping,
            ),
        ));
    }

    pub fn bind_basic_block(&mut self, basic_block_index: usize) {
        match &mut self.guest_basic_block_labels[basic_block_index] {
            None => unreachable!(),
            Some(label) => self.masm.bind(label),
        }
    }

    pub fn b_basic_block(&mut self, basic_block_index: usize) {
        match &mut self.guest_basic_block_labels[basic_block_index] {
            None => unreachable!(),
            Some(label) => self.masm.b2(label, BranchHint_kNear),
        }
    }
}

impl Deref for BlockAsm {
    type Target = MacroAssembler;

    fn deref(&self) -> &Self::Target {
        &self.masm
    }
}

impl DerefMut for BlockAsm {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.masm
    }
}

use crate::core::emu::Emu;
use crate::core::CpuType;
use crate::jit::assembler::reg_alloc::{RegAlloc, GUEST_REGS_LENGTH, GUEST_REG_ALLOCATIONS};
use crate::jit::inst_info::{InstInfo, Operands, Shift, ShiftValue};
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::{inst_info, Cond};
use crate::mmap::{PAGE_SHIFT, PAGE_SIZE};
use std::ops::{Deref, DerefMut};
use std::ptr;
use vixl::{
    BranchHint_kNear, FlagsUpdate, FlagsUpdate_DontCare, InstructionSet_A32, InstructionSet_T32, Label, MacroAssembler, MaskedSpecialRegisterType_CPSR_f, MasmAdd5, MasmB2, MasmBlx1, MasmLdr2,
    MasmLdr3, MasmLsr5, MasmMov2, MasmMrs2, MasmMsr2, MasmNop, MasmPop1, MasmPush1, MasmStr3, MasmStrb2, MasmSub5, ShiftType_ASR, ShiftType_LSL, ShiftType_LSR, ShiftType_ROR, ShiftType_RRX,
    SpecialRegisterType_CPSR,
};

pub const GUEST_REGS_PTR_REG: Reg = Reg::R3;
pub const CPSR_TMP_REG: Reg = Reg::R0;

#[derive(Copy, Clone)]
pub struct GuestInstMetadataFastMem {
    pub start_offset: u16,
    pub size: u16,
    pub op: Op,
    pub operands: Operands,
    pub op0: Reg,
    pub opcode_offset: usize,
    pub is_os_irq_handler: bool,
}

impl GuestInstMetadataFastMem {
    fn new(start_offset: u16, size: u16, op: Op, operands: Operands, op0: Reg, opcode_offset: usize, is_os_irq_handler: bool) -> Self {
        GuestInstMetadataFastMem {
            start_offset,
            size,
            op,
            operands,
            op0,
            opcode_offset,
            is_os_irq_handler,
        }
    }
}

#[derive(Copy, Clone)]
pub struct GuestInstMetadataSlowMem {
    pub initial_patch_addr: u32,
    pub io_func: *const (),
}

#[derive(Copy, Clone)]
pub union GuestInstMetadataShared {
    pub fast: GuestInstMetadataFastMem,
    pub slow: GuestInstMetadataSlowMem,
}

impl GuestInstMetadataShared {
    fn new(fast: GuestInstMetadataFastMem) -> Self {
        GuestInstMetadataShared { fast }
    }
}

#[derive(Clone)]
pub struct GuestInstMetadata {
    pub s: GuestInstMetadataShared,
    pub pc: u32,
    pub total_cycle_count: u16,
    pub dirty_guest_regs: RegReserve,
    pub mapped_guest_regs: [Reg; GUEST_REGS_LENGTH],
}

impl GuestInstMetadata {
    pub fn new(
        fast_mem_start_offset: u16,
        fast_mem_size: u16,
        opcode_offset: usize,
        is_os_irq_handler: bool,
        pc: u32,
        total_cycle_count: u16,
        op: Op,
        operands: Operands,
        op0: Reg,
        dirty_guest_regs: RegReserve,
        mapped_guest_regs: [Reg; GUEST_REGS_LENGTH],
    ) -> Self {
        GuestInstMetadata {
            s: GuestInstMetadataShared::new(GuestInstMetadataFastMem::new(fast_mem_start_offset, fast_mem_size, op, operands, op0, opcode_offset, is_os_irq_handler)),
            pc,
            total_cycle_count,
            dirty_guest_regs,
            mapped_guest_regs,
        }
    }
}

#[repr(C)]
pub struct GuestInstOffset {
    pub offset: u16,
    pub pre_cycle_count_sum: u16,
    pub mapping: [*const u32; GUEST_REG_ALLOCATIONS.len()],
    pub pc: u32,
}

impl GuestInstOffset {
    fn new(offset: u16, pre_cycle_count_sum: u16, pc: u32) -> Self {
        GuestInstOffset {
            offset,
            mapping: [ptr::null(); GUEST_REG_ALLOCATIONS.len()],
            pre_cycle_count_sum,
            pc,
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
    guest_start: usize,
    pub guest_inst_offsets: Vec<GuestInstOffset>,
    pub guest_basic_block_labels: Vec<Option<Label>>,
    basic_blocks_guest_regs_mappings: Vec<(RegReserve, RegReserve, [Reg; GUEST_REGS_LENGTH])>,
    last_pc_value: u32,
    guest_regs_ptr: *mut u32,
    is_os_irq_handler: bool,
    pub is_fs_clear_overlay: bool,
    pub jit_entry_insert_locations: Vec<(Reg, usize)>,
}

impl BlockAsm {
    pub fn new(cpu: CpuType, thumb: bool, is_os_irq_handler: bool) -> Self {
        BlockAsm {
            masm: MacroAssembler::new(if thumb { InstructionSet_T32 } else { InstructionSet_A32 }),
            reg_alloc: RegAlloc::new(thumb),
            current_pc: 0,
            thumb,
            dirty_guest_regs: RegReserve::new(),
            guest_inst_metadata: Vec::new(),
            guest_start: 0,
            guest_inst_offsets: Vec::new(),
            guest_basic_block_labels: Vec::new(),
            basic_blocks_guest_regs_mappings: Vec::new(),
            last_pc_value: 0,
            guest_regs_ptr: cpu.guest_regs_addr() as _,
            is_os_irq_handler,
            is_fs_clear_overlay: false,
            jit_entry_insert_locations: Vec::new(),
        }
    }

    pub fn prologue(&mut self, basic_block_len: usize) {
        self.push1(reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::LR));
        self.sub5(FlagsUpdate_DontCare, Cond::AL, Reg::SP, Reg::SP, &4.into());

        self.guest_basic_block_labels.resize_with(basic_block_len, || None);
        self.basic_blocks_guest_regs_mappings
            .resize_with(basic_block_len, || (reg_reserve!(), reg_reserve!(), [Reg::None; GUEST_REGS_LENGTH]));
    }

    pub fn restore_stack(&mut self) {
        self.add5(FlagsUpdate_DontCare, Cond::AL, Reg::SP, Reg::SP, &4.into());
        self.pop1(reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::LR));
    }

    pub fn exit_guest_context(&mut self, host_sp_ptr: *mut usize) {
        self.ldr2(Reg::R0, host_sp_ptr as u32);
        self.ldr2(Reg::SP, &Reg::R0.into());
        self.pop1(reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::R12, Reg::PC));
    }

    pub fn init_guest_regs_mapping(&mut self, guest_regs: RegReserve, output_regs: RegReserve, basic_block_index: usize) {
        self.reg_alloc = RegAlloc::new(self.thumb);
        let dirty_guest_regs = self.reg_alloc.reserve_guest_regs(guest_regs - Reg::CPSR, false, &mut self.masm);
        self.basic_blocks_guest_regs_mappings[basic_block_index] = (dirty_guest_regs & output_regs, self.get_free_host_regs(), self.reg_alloc.guest_regs_mapping);
    }

    pub fn init_guest_regs(&mut self, guest_regs: RegReserve, basic_block_index: usize) {
        let (dirty_guest_regs, free_regs, guest_regs_mapping) = self.basic_blocks_guest_regs_mappings[basic_block_index];
        self.dirty_guest_regs = dirty_guest_regs;
        self.set_guest_regs_mapping(guest_regs_mapping);
        self.reg_alloc.free_regs = free_regs;

        if basic_block_index == 0 {
            self.restore_guest_regs_ptr();
        }

        if basic_block_index == 0 && guest_regs.is_reserved(Reg::CPSR) {
            self.load_guest_reg(CPSR_TMP_REG, Reg::CPSR);
            self.msr2(MaskedSpecialRegisterType_CPSR_f.into(), &CPSR_TMP_REG.into());
        }
    }

    pub fn set_guest_start(&mut self) {
        self.guest_start = self.get_cursor_offset() as usize;
    }

    pub fn guest_offset(&mut self, pre_cycle_count_sum: u16, cpu: CpuType, emu: &Emu) {
        let offset = self.get_cursor_offset() as usize - self.guest_start;
        let offset = offset - if self.thumb { 2 } else { 4 };
        debug_assert!(offset <= u16::MAX as usize);
        self.guest_inst_offsets.push(GuestInstOffset::new(offset as u16, pre_cycle_count_sum, self.last_pc_value));
        let inst_offset = self.guest_inst_offsets.last_mut().unwrap();
        for (i, guest_reg) in self.reg_alloc.host_regs_mapping.iter().enumerate() {
            match guest_reg {
                Reg::PC | Reg::None => inst_offset.mapping[i] = &inst_offset.pc,
                _ => inst_offset.mapping[i] = emu.thread_get_reg(cpu, *guest_reg) as _,
            }
        }
    }

    pub fn call(&mut self, fun: *const ()) {
        self.ldr2(Reg::R12, fun as u32);
        self.blx1(Reg::R12);
    }

    pub fn alloc_guest_inst(&mut self, inst: &InstInfo, next_live_regs: RegReserve) {
        let mut input_regs = inst.src_regs;
        let output_regs = inst.out_regs;
        if inst.cond != Cond::AL {
            input_regs += output_regs - Reg::PC;
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

            self.last_pc_value = pc;
            self.ldr2(pc_reg, pc);
        }
    }

    pub fn alloc_guest_regs(&mut self, input_regs: RegReserve, output_regs: RegReserve, cond: Cond, next_live_regs: RegReserve) {
        let spilled_guest_regs = self
            .reg_alloc
            .alloc_guest_regs(input_regs - Reg::CPSR, output_regs - Reg::CPSR, next_live_regs, self.dirty_guest_regs, &mut self.masm);
        if cond == Cond::AL {
            self.dirty_guest_regs -= spilled_guest_regs;
        }
    }

    pub fn load_guest_reg_cond(&mut self, cond: Cond, dest_reg: Reg, guest_reg: Reg) {
        self.ldr3(cond, dest_reg, &(GUEST_REGS_PTR_REG, guest_reg as i32 * 4).into());
    }

    pub fn load_guest_reg(&mut self, dest_reg: Reg, guest_reg: Reg) {
        self.load_guest_reg_cond(Cond::AL, dest_reg, guest_reg);
    }

    pub fn store_guest_reg_cond(&mut self, cond: Cond, src_reg: Reg, guest_reg: Reg) {
        self.str3(cond, src_reg, &(GUEST_REGS_PTR_REG, guest_reg as i32 * 4).into());
    }

    pub fn store_guest_reg(&mut self, src_reg: Reg, guest_reg: Reg) {
        self.store_guest_reg_cond(Cond::AL, src_reg, guest_reg);
    }

    pub fn load_guest_cpsr_reg(&mut self, tmp_reg: Reg) {
        self.load_guest_reg(tmp_reg, Reg::CPSR);
        self.msr2(MaskedSpecialRegisterType_CPSR_f.into(), &tmp_reg.into());
    }

    pub fn store_guest_cpsr_reg(&mut self, flags_update: FlagsUpdate, tmp_reg: Reg) {
        self.mrs2(tmp_reg, SpecialRegisterType_CPSR.into());
        self.lsr5(flags_update, Cond::AL, tmp_reg, tmp_reg, &24.into());
        self.strb2(tmp_reg, &(GUEST_REGS_PTR_REG, Reg::CPSR as i32 * 4 + 3).into());
    }

    pub fn save_dirty_guest_cpsr(&mut self, clear: bool) {
        if self.dirty_guest_regs.is_reserved(Reg::CPSR) {
            self.store_guest_cpsr_reg(FlagsUpdate_DontCare, CPSR_TMP_REG);
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

    pub fn set_guest_regs_mapping(&mut self, guest_regs_mapping: [Reg; GUEST_REGS_LENGTH]) {
        self.reg_alloc.set_guest_regs_mappings(&guest_regs_mapping);
    }

    pub fn get_guest_regs_mapping(&self) -> [Reg; GUEST_REGS_LENGTH] {
        self.reg_alloc.guest_regs_mapping
    }

    pub fn save_guest_regs(&mut self, regs: RegReserve) {
        self.reg_alloc.save_active_guest_regs(regs - Reg::CPSR, &mut self.masm);
        if regs.is_reserved(Reg::CPSR) {
            self.store_guest_cpsr_reg(FlagsUpdate_DontCare, CPSR_TMP_REG);
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
        let guest_regs_ptr = self.guest_regs_ptr as u32;
        self.mov2(GUEST_REGS_PTR_REG, &guest_regs_ptr.into());
    }

    pub fn restore_tmp_regs(&mut self, next_live_regs: RegReserve) {
        self.restore_guest_regs_ptr();
        if next_live_regs.is_reserved(Reg::CPSR) {
            self.load_guest_cpsr_reg(CPSR_TMP_REG);
        }
    }

    pub fn reload_active_guest_regs(&mut self, guest_regs: RegReserve) {
        self.reg_alloc.reload_active_guest_regs(guest_regs - Reg::CPSR, &mut self.masm);
    }

    pub fn reload_active_guest_regs_all(&mut self) {
        self.reg_alloc.reload_active_guest_regs(RegReserve::all() - Reg::PC, &mut self.masm);
    }

    pub fn get_guest_inst_metadata_len(&self) -> usize {
        self.guest_inst_metadata.len()
    }

    pub fn guest_inst_metadata(&mut self, total_cycles_reg: u16, inst: &InstInfo, fast_mem_start: u32, op0: Reg, mut dirty_guest_regs: RegReserve) {
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
                (offset - fast_mem_start) as u16,
                0,
                offset as usize & (PAGE_SIZE - 1),
                self.is_os_irq_handler,
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

    pub fn set_fast_mem_size(&mut self, start: usize, size: u16) {
        for i in start..self.guest_inst_metadata.len() {
            self.guest_inst_metadata[i].1.s.fast.size = size;
        }
    }

    pub fn set_fast_mem_size_last(&mut self, size: u16) {
        self.guest_inst_metadata.last_mut().unwrap().1.s.fast.size = size;
    }

    pub fn bind_basic_block(&mut self, basic_block_index: usize) {
        self.masm.bind(self.guest_basic_block_labels[basic_block_index].as_mut().unwrap());
    }

    pub fn b_basic_block(&mut self, basic_block_index: usize) {
        self.masm.b2(self.guest_basic_block_labels[basic_block_index].as_mut().unwrap(), BranchHint_kNear);
    }

    pub fn init_basic_block_regs(&mut self, input_regs: RegReserve, basic_block_index: usize) {
        self.reg_alloc.guest_regs_mapping = self.basic_blocks_guest_regs_mappings[basic_block_index].2;
        self.reload_active_guest_regs_all();
        if input_regs.is_reserved(Reg::CPSR) {
            self.load_guest_cpsr_reg(CPSR_TMP_REG);
        }
    }

    pub fn relocate_for_basic_block(&mut self, flags_update: FlagsUpdate, basic_block_output_regs: RegReserve, basic_block_index: usize) {
        self.reg_alloc.relocate_guest_regs(
            flags_update,
            self.dirty_guest_regs - Reg::PC - Reg::CPSR,
            basic_block_output_regs,
            &self.basic_blocks_guest_regs_mappings[basic_block_index].2,
            &mut self.masm,
        );
    }

    pub fn insert_jit_entry(&mut self, reg: Reg) {
        self.ensure_emit_for(64);
        let offset = self.get_cursor_offset();
        self.jit_entry_insert_locations.push((reg, offset as usize));
        self.nop0();
        self.nop0();
        if self.thumb {
            self.nop0();
            self.nop0();
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

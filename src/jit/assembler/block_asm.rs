use crate::core::thread_regs::ThreadRegs;
use crate::jit::assembler::reg_allocator::{RegAllocator, VReg, GUEST_REGS_COUNT, V_REGS_COUNT};
use crate::jit::assembler::vixl::vixl::{MaskedSpecialRegister, MaskedSpecialRegisterType_CPSR_f, MemOperand, Operand};
use crate::jit::assembler::vixl::{MacroAssembler, MasmAdd3, MasmBlx1, MasmLdr2, MasmMsr2, MasmPush1, MasmSub3};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use std::ops::{Deref, DerefMut};

const REGISTERS_TO_SAVE: RegReserve = reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::R12, Reg::LR);

pub struct BlockAsm {
    masm: MacroAssembler,
    reg_alloc: RegAllocator,
    guest_regs_ptr_vreg: VReg,
}

impl BlockAsm {
    pub fn new() -> Self {
        BlockAsm {
            masm: MacroAssembler::new(),
            reg_alloc: RegAllocator::new(),
            guest_regs_ptr_vreg: VReg::default(),
        }
    }

    pub fn init(&mut self, guest_regs_ptr: *mut u32) {
        self.push1(REGISTERS_TO_SAVE);
        self.sub3(Reg::SP, Reg::SP, &((GUEST_REGS_COUNT + V_REGS_COUNT) as u32).into());

        self.guest_regs_ptr_vreg = self.reg_alloc.alloc_vreg();
        let guest_regs_ptr_reg = self.get_guest_regs_ptr_reg(false);
        self.ldr2(guest_regs_ptr_reg, guest_regs_ptr as u32);
    }

    fn get_guest_regs_ptr_reg(&mut self, needs_restore: bool) -> Reg {
        self.reg_alloc.map_vreg(self.guest_regs_ptr_vreg, needs_restore, &mut self.masm)
    }

    pub fn spill_regs(&mut self, regs: RegReserve) {
        for reg in regs {
            self.spill_reg(reg);
        }
    }

    pub fn spill_reg(&mut self, reg: Reg) {
        self.reg_alloc.spill_reg(reg, &mut self.masm);
    }

    pub fn spill_regs_for_call(&mut self) {
        self.spill_regs(reg_reserve!(Reg::R0, Reg::R1, Reg::R2, Reg::R3))
    }

    pub fn map_guest_reg(&mut self, guest_reg: Reg, needs_restore: bool) -> Reg {
        self.reg_alloc.map_guest_reg(guest_reg, needs_restore, &mut self.masm)
    }

    pub fn call(&mut self, fun: *const ()) {
        self.reg_alloc.spill_reg(Reg::R12, &mut self.masm);
        self.ldr2(Reg::R12, fun as u32);
        self.blx1(Reg::R12);
    }

    pub fn restore_nzcv(&mut self) {
        let cpsr_vreg = self.reg_alloc.alloc_vreg();
        let cpsr_reg = self.reg_alloc.map_vreg_scratch(cpsr_vreg, false, &mut self.masm);
        self.ldr_guest_reg(cpsr_reg, Reg::CPSR);
        self.msr2(MaskedSpecialRegisterType_CPSR_f.into(), &cpsr_reg.into());
        self.reg_alloc.free_vreg(cpsr_vreg);
    }

    pub fn ldr_guest_reg(&mut self, dest_reg: Reg, guest_reg: Reg) {
        let guest_regs_ptr_reg = self.get_guest_regs_ptr_reg(true);
        self.ldr2(dest_reg, &MemOperand::reg_offset(guest_regs_ptr_reg, guest_reg as i32 * 4));
    }
}

impl AsRef<MacroAssembler> for BlockAsm {
    fn as_ref(&self) -> &MacroAssembler {
        &self.masm
    }
}

impl AsMut<MacroAssembler> for BlockAsm {
    fn as_mut(&mut self) -> &mut MacroAssembler {
        &mut self.masm
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

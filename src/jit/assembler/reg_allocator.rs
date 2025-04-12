use crate::jit::assembler::vixl::vixl::MemOperand;
use crate::jit::assembler::vixl::{MacroAssembler, MasmLdr2, MasmStr2};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};

pub const GUEST_REGS_COUNT: usize = 15;
pub const V_REGS_COUNT: usize = 13;

#[derive(Copy, Clone, Default)]
pub struct VReg(Reg);

pub struct RegAllocator {
    guest_reg_map: [Reg; 2],
    spilled_guest_regs: RegReserve,
    unspillable_guest_regs: RegReserve,
    active_guest_regs: RegReserve,

    allocated_vregs: RegReserve,
    mapped_vregs: [Reg; V_REGS_COUNT],
    spilled_vregs: RegReserve,
}

impl RegAllocator {
    pub fn new() -> Self {
        RegAllocator {
            guest_reg_map: [Reg::None; 2],
            active_guest_regs: RegReserve::new(),
            spilled_guest_regs: RegReserve::new(),
            unspillable_guest_regs: RegReserve::new(),

            allocated_vregs: RegReserve::new(),
            mapped_vregs: [Reg::None; V_REGS_COUNT],
            spilled_vregs: RegReserve::new(),
        }
    }

    fn spillable_guest_regs(&self) -> RegReserve {
        self.active_guest_regs - self.unspillable_guest_regs
    }

    pub fn set_unspillable_guest_regs(&mut self, regs: RegReserve) {
        self.unspillable_guest_regs = regs;
    }

    pub fn free_vreg(&mut self, reg: VReg) {
        self.allocated_vregs -= reg.0;
        self.mapped_vregs[reg.0 as usize] = Reg::None;
        self.spilled_vregs -= reg.0;
    }

    pub fn spill_reg(&mut self, reg: Reg, masm: &mut MacroAssembler) {
        if self.active_guest_regs.is_reserved(reg) {
            self.spill_guest_reg(reg, masm);
        } else {
            for (i, mapped_reg) in self.mapped_vregs.iter_mut().enumerate() {
                if *mapped_reg == reg {
                    self.spill_vreg(VReg(Reg::from(i as u8)), masm);
                    break;
                }
            }
        }
    }

    pub fn alloc_vreg(&mut self) -> VReg {
        let free_vregs = !self.allocated_vregs;
        debug_assert!(!free_vregs.is_empty());
        let reg = free_vregs.peek_gp().unwrap();
        self.allocated_vregs += reg;
        VReg(reg)
    }

    pub fn map_vreg(&mut self, vreg: VReg, needs_restore: bool, masm: &mut MacroAssembler) -> Reg {
        self.map_vreg_internal(vreg, needs_restore, reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11), masm)
    }

    pub fn map_vreg_scratch(&mut self, vreg: VReg, needs_restore: bool, masm: &mut MacroAssembler) -> Reg {
        self.map_vreg_internal(vreg, needs_restore, RegReserve::gp() + Reg::LR, masm)
    }

    fn map_vreg_internal(&mut self, vreg: VReg, needs_restore: bool, allowed_regs: RegReserve, masm: &mut MacroAssembler) -> Reg {
        debug_assert!(self.allocated_vregs.is_reserved(vreg.0));
        match self.mapped_vregs[vreg.0 as usize] {
            Reg::None => {
                let reg = self.alloc_reg(allowed_regs, masm);
                self.mapped_vregs[vreg.0 as usize] = reg;
                if needs_restore && self.spilled_vregs.is_reserved(vreg.0) {
                    self.restore_vreg(vreg, masm);
                }
                reg
            }
            reg => {
                if allowed_regs.is_reserved(reg) {
                    reg
                } else {
                    if needs_restore {
                        self.spill_vreg(vreg, masm);
                    } else {
                        self.mapped_vregs[vreg.0 as usize] = Reg::None;
                    }
                    self.map_vreg_internal(vreg, needs_restore, allowed_regs, masm)
                }
            }
        }
    }

    fn alloc_reg(&mut self, allowed_regs: RegReserve, masm: &mut MacroAssembler) -> Reg {
        let mut free_regs = allowed_regs;
        for reg in &self.mapped_vregs {
            free_regs -= *reg;
        }

        if !free_regs.is_empty() {
            return free_regs.peek_gp().unwrap();
        }

        todo!()
    }

    pub fn map_guest_reg(&mut self, guest_reg: Reg, needs_restore: bool, masm: &mut MacroAssembler) -> Reg {
        if guest_reg == Reg::SP {
            todo!()
        } else {
            for (i, mapped_reg) in self.mapped_vregs.iter_mut().enumerate() {
                if *mapped_reg == guest_reg {
                    self.spill_vreg(VReg(Reg::from(i as u8)), masm);
                    break;
                }
            }
            if needs_restore && self.spilled_guest_regs.is_reserved(guest_reg) {
                todo!()
            }
            guest_reg
        }
    }

    fn restore_vreg(&mut self, vreg: VReg, masm: &mut MacroAssembler) {
        self.spilled_vregs -= vreg.0;
        let reg = self.mapped_vregs[vreg.0 as usize];
        masm.ldr2(reg, &MemOperand::reg_offset(Reg::SP, (reg as i32 + GUEST_REGS_COUNT as i32) * 4));
    }

    fn spill_vreg(&mut self, vreg: VReg, masm: &mut MacroAssembler) {
        self.spilled_vregs += vreg.0;
        let reg = &mut self.mapped_vregs[vreg.0 as usize];
        masm.str2(*reg, &MemOperand::reg_offset(Reg::SP, (*reg as i32 + GUEST_REGS_COUNT as i32) * 4));
        *reg = Reg::None;
    }

    fn spill_guest_reg(&mut self, guest_reg: Reg, masm: &mut MacroAssembler) {
        self.spilled_guest_regs += guest_reg;
        self.active_guest_regs -= guest_reg;
        masm.str2(guest_reg, &MemOperand::reg_offset(Reg::SP, guest_reg as i32 * 4));
    }
}

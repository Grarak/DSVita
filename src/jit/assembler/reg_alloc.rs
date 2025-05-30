use crate::jit::assembler::block_asm::GUEST_REGS_PTR_REG;
use crate::jit::assembler::vixl::vixl::MemOperand;
use crate::jit::assembler::vixl::{MacroAssembler, MasmLdr2, MasmStr2};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::logging::debug_panic;

pub const GUEST_REG_ALLOCATIONS: RegReserve = reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11);
pub const GUEST_REGS_LENGTH: usize = Reg::PC as usize + 1;

pub struct RegAlloc {
    pub free_regs: RegReserve,
    pub guest_regs_mapping: [Reg; GUEST_REGS_LENGTH],
    thumb: bool,
}

impl RegAlloc {
    pub fn new(thumb: bool) -> Self {
        RegAlloc {
            free_regs: GUEST_REG_ALLOCATIONS,
            guest_regs_mapping: [Reg::None; GUEST_REGS_LENGTH],
            thumb,
        }
    }

    fn restore_guest_reg(&mut self, guest_reg: Reg, dest_reg: Reg, masm: &mut MacroAssembler) {
        masm.ldr2(dest_reg, &MemOperand::reg_offset(GUEST_REGS_PTR_REG, guest_reg as i32 * 4));
    }

    fn spill_guest_reg(&mut self, guest_reg: Reg, src_reg: Reg, masm: &mut MacroAssembler) {
        masm.str2(src_reg, &MemOperand::reg_offset(GUEST_REGS_PTR_REG, guest_reg as i32 * 4));
    }

    fn alloc_guest_reg(&mut self, guest_reg: Reg, is_input: bool, used_regs: RegReserve, next_live_regs: RegReserve, dirty_guest_regs: RegReserve, masm: &mut MacroAssembler) -> (Reg, Reg) {
        for reg in self.free_regs {
            if self.thumb && reg.is_low() != guest_reg.is_low() {
                continue;
            }
            self.guest_regs_mapping[guest_reg as usize] = reg;
            self.free_regs -= reg;
            if is_input && guest_reg != Reg::PC {
                self.restore_guest_reg(guest_reg, reg, masm);
            }
            return (reg, Reg::None);
        }

        for reg in 0..self.guest_regs_mapping.len() {
            let mapped_reg = self.guest_regs_mapping[reg];
            let reg = Reg::from(reg as u8);
            if mapped_reg != Reg::None && !next_live_regs.is_reserved(reg) && !used_regs.is_reserved(reg) && (!self.thumb || reg.is_low() == guest_reg.is_low()) {
                self.guest_regs_mapping[guest_reg as usize] = mapped_reg;
                self.guest_regs_mapping[reg as usize] = Reg::None;
                if dirty_guest_regs.is_reserved(reg) {
                    self.spill_guest_reg(reg, mapped_reg, masm);
                }
                if is_input && guest_reg != Reg::PC {
                    self.restore_guest_reg(guest_reg, mapped_reg, masm);
                }
                return (mapped_reg, reg);
            }
        }

        for reg in 0..self.guest_regs_mapping.len() {
            let mapped_reg = self.guest_regs_mapping[reg];
            let reg = Reg::from(reg as u8);
            if mapped_reg != Reg::None && !used_regs.is_reserved(reg) && (!self.thumb || reg.is_low() == guest_reg.is_low()) {
                self.guest_regs_mapping[guest_reg as usize] = mapped_reg;
                self.guest_regs_mapping[reg as usize] = Reg::None;
                if dirty_guest_regs.is_reserved(reg) {
                    self.spill_guest_reg(reg, mapped_reg, masm);
                }
                if is_input && guest_reg != Reg::PC {
                    self.restore_guest_reg(guest_reg, mapped_reg, masm);
                }
                return (mapped_reg, reg);
            }
        }

        let mut mapped_regs = RegReserve::new();
        for reg in 0..self.guest_regs_mapping.len() {
            let mapped_reg = self.guest_regs_mapping[reg];
            if mapped_reg != Reg::None {
                mapped_regs += Reg::from(reg as u8);
            }
        }
        debug_panic!("No free regs available for allocating guest mapping, used regs: {used_regs:?} mapped guest regs: {mapped_regs:?}");
    }

    pub fn alloc_guest_regs(&mut self, input_regs: RegReserve, output_regs: RegReserve, next_live_regs: RegReserve, dirty_guest_regs: RegReserve, masm: &mut MacroAssembler) -> RegReserve {
        let mut spilled_regs = RegReserve::new();
        let used_regs = input_regs + output_regs;
        for input_reg in input_regs {
            if self.guest_regs_mapping[input_reg as usize] == Reg::None {
                let (_, spilled_reg) = self.alloc_guest_reg(input_reg, true, used_regs, next_live_regs, dirty_guest_regs, masm);
                if spilled_reg != Reg::None {
                    spilled_regs += spilled_reg;
                }
            }
        }

        for output_reg in output_regs {
            if self.guest_regs_mapping[output_reg as usize] == Reg::None {
                let (_, spilled_reg) = self.alloc_guest_reg(output_reg, false, used_regs, next_live_regs, dirty_guest_regs, masm);
                if spilled_reg != Reg::None {
                    spilled_regs += spilled_reg;
                }
            }
        }

        spilled_regs
    }

    pub fn get_guest_map(&self, reg: Reg) -> Reg {
        self.guest_regs_mapping[reg as usize]
    }

    pub fn save_active_guest_regs(&mut self, guest_regs: RegReserve, masm: &mut MacroAssembler) {
        for guest_reg in guest_regs {
            let mapped_reg = self.guest_regs_mapping[guest_reg as usize];
            if mapped_reg != Reg::None {
                masm.str2(mapped_reg, &MemOperand::reg_offset(GUEST_REGS_PTR_REG, guest_reg as i32 * 4));
            }
        }
    }

    pub fn reload_active_guest_regs(&mut self, guest_regs: RegReserve, masm: &mut MacroAssembler) {
        for guest_reg in guest_regs {
            let mapped_reg = self.guest_regs_mapping[guest_reg as usize];
            if mapped_reg != Reg::None {
                self.restore_guest_reg(guest_reg, mapped_reg, masm);
            }
        }
    }
}

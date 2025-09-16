use crate::jit::assembler::block_asm::GUEST_REGS_PTR_REG;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::logging::debug_panic;
use vixl::{Cond, FlagsUpdate, MacroAssembler, MasmLdr2, MasmMov4, MasmStr2};

pub const GUEST_REG_ALLOCATIONS: RegReserve = reg_reserve!(Reg::R4, Reg::R5, Reg::R6, Reg::R7, Reg::R8, Reg::R9, Reg::R10, Reg::R11);
pub const GUEST_REGS_LENGTH: usize = Reg::PC as usize + 1;

pub struct RegAlloc {
    pub free_regs: RegReserve,
    pub guest_regs_mapping: [Reg; GUEST_REGS_LENGTH],
    pub host_regs_mapping: [Reg; GUEST_REG_ALLOCATIONS.len()],
    thumb: bool,
}

impl RegAlloc {
    pub fn new(thumb: bool) -> Self {
        RegAlloc {
            free_regs: GUEST_REG_ALLOCATIONS,
            guest_regs_mapping: [Reg::None; GUEST_REGS_LENGTH],
            host_regs_mapping: [Reg::None; GUEST_REG_ALLOCATIONS.len()],
            thumb,
        }
    }

    pub fn set_guest_regs_mappings(&mut self, guest_regs_mapping: &[Reg; GUEST_REGS_LENGTH]) {
        self.guest_regs_mapping = *guest_regs_mapping;
        self.host_regs_mapping = [Reg::None; GUEST_REG_ALLOCATIONS.len()];
        for (i, &mapped_reg) in self.guest_regs_mapping.iter().enumerate() {
            if mapped_reg != Reg::None {
                self.host_regs_mapping[mapped_reg as usize - 4] = Reg::from(i as u8);
            }
        }
    }

    fn set_guest_reg_mapping(&mut self, guest_reg: Reg, mapped_reg: Reg) {
        let guest_mapped_reg = self.guest_regs_mapping[guest_reg as usize];
        self.guest_regs_mapping[guest_reg as usize] = mapped_reg;
        if mapped_reg == Reg::None {
            self.host_regs_mapping[guest_mapped_reg as usize - 4] = Reg::None;
        } else {
            self.host_regs_mapping[mapped_reg as usize - 4] = guest_reg;
        }
    }

    fn restore_guest_reg(guest_reg: Reg, dest_reg: Reg, masm: &mut MacroAssembler) {
        masm.ldr2(dest_reg, &(GUEST_REGS_PTR_REG, guest_reg as i32 * 4).into());
    }

    fn spill_guest_reg(guest_reg: Reg, src_reg: Reg, masm: &mut MacroAssembler) {
        masm.str2(src_reg, &(GUEST_REGS_PTR_REG, guest_reg as i32 * 4).into());
    }

    pub fn reserve_guest_regs(&mut self, guest_regs: RegReserve, restore: bool, masm: &mut MacroAssembler) -> RegReserve {
        let mut reserved_regs = reg_reserve!();
        for reg in guest_regs {
            if self.alloc_free_guest_reg(reg, restore, masm) != Reg::None {
                reserved_regs += reg;
            }
        }
        reserved_regs
    }

    fn alloc_free_guest_reg(&mut self, guest_reg: Reg, is_input: bool, masm: &mut MacroAssembler) -> Reg {
        for reg in self.free_regs {
            if self.thumb && reg.is_low() != guest_reg.is_low() {
                continue;
            }
            self.set_guest_reg_mapping(guest_reg, reg);
            self.free_regs -= reg;
            if is_input && guest_reg != Reg::PC {
                Self::restore_guest_reg(guest_reg, reg, masm);
            }
            return reg;
        }
        Reg::None
    }

    fn alloc_guest_reg(&mut self, guest_reg: Reg, is_input: bool, used_regs: RegReserve, next_live_regs: RegReserve, dirty_guest_regs: RegReserve, masm: &mut MacroAssembler) -> (Reg, Reg) {
        let reg = self.alloc_free_guest_reg(guest_reg, is_input, masm);
        if reg != Reg::None {
            return (reg, Reg::None);
        }

        for reg in 0..self.guest_regs_mapping.len() {
            let mapped_reg = self.guest_regs_mapping[reg];
            let reg = Reg::from(reg as u8);
            if mapped_reg != Reg::None && !next_live_regs.is_reserved(reg) && !used_regs.is_reserved(reg) && (!self.thumb || reg.is_low() == guest_reg.is_low()) {
                self.set_guest_reg_mapping(reg, Reg::None);
                self.set_guest_reg_mapping(guest_reg, mapped_reg);
                if dirty_guest_regs.is_reserved(reg) {
                    Self::spill_guest_reg(reg, mapped_reg, masm);
                }
                if is_input && guest_reg != Reg::PC {
                    Self::restore_guest_reg(guest_reg, mapped_reg, masm);
                }
                return (mapped_reg, reg);
            }
        }

        for reg in 0..self.guest_regs_mapping.len() {
            let mapped_reg = self.guest_regs_mapping[reg];
            let reg = Reg::from(reg as u8);
            if mapped_reg != Reg::None && !used_regs.is_reserved(reg) && (!self.thumb || reg.is_low() == guest_reg.is_low()) {
                self.set_guest_reg_mapping(reg, Reg::None);
                self.set_guest_reg_mapping(guest_reg, mapped_reg);
                if dirty_guest_regs.is_reserved(reg) {
                    Self::spill_guest_reg(reg, mapped_reg, masm);
                }
                if is_input && guest_reg != Reg::PC {
                    Self::restore_guest_reg(guest_reg, mapped_reg, masm);
                }
                return (mapped_reg, reg);
            }
        }

        let mut mapped_regs = RegReserve::new();
        for guest_reg in self.host_regs_mapping {
            if guest_reg != Reg::None {
                mapped_regs += guest_reg;
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

    pub fn save_active_guest_regs(&self, guest_regs: RegReserve, masm: &mut MacroAssembler) {
        for guest_reg in guest_regs {
            let mapped_reg = self.guest_regs_mapping[guest_reg as usize];
            if mapped_reg != Reg::None {
                Self::spill_guest_reg(guest_reg, mapped_reg, masm);
            }
        }
    }

    pub fn reload_active_guest_regs(&self, guest_regs: RegReserve, masm: &mut MacroAssembler) {
        for guest_reg in guest_regs {
            let mapped_reg = self.guest_regs_mapping[guest_reg as usize];
            if mapped_reg != Reg::None {
                Self::restore_guest_reg(guest_reg, mapped_reg, masm);
            }
        }
    }

    fn swap_guest_regs(
        &mut self,
        flags_update: FlagsUpdate,
        root_guest_reg: Reg,
        guest_reg: Reg,
        desired_host_reg: Reg,
        desired_guest_regs_mapping: &[Reg; GUEST_REGS_LENGTH],
        masm: &mut MacroAssembler,
    ) {
        let current_host_reg = self.guest_regs_mapping[guest_reg as usize];
        if current_host_reg == desired_host_reg {
            return;
        }

        let current_host_used_by = self.host_regs_mapping[desired_host_reg as usize - 4];
        if current_host_used_by != Reg::None {
            let next_desired_host_reg = desired_guest_regs_mapping[current_host_used_by as usize];
            if root_guest_reg == current_host_used_by {
                masm.mov4(flags_update, Cond::AL, Reg::R2, &desired_host_reg.into());
                self.guest_regs_mapping[current_host_used_by as usize] = Reg::R2;
            } else if next_desired_host_reg != Reg::None {
                self.swap_guest_regs(flags_update, root_guest_reg, current_host_used_by, next_desired_host_reg, desired_guest_regs_mapping, masm);
            }
        }

        let current_host_reg = self.guest_regs_mapping[guest_reg as usize];
        self.set_guest_reg_mapping(guest_reg, desired_host_reg);
        masm.mov4(flags_update, Cond::AL, desired_host_reg, &current_host_reg.into());
    }

    pub fn relocate_guest_regs(
        &mut self,
        flags_update: FlagsUpdate,
        dirty_guest_regs: RegReserve,
        basic_block_output_regs: RegReserve,
        desired_guest_regs_mapping: &[Reg; GUEST_REGS_LENGTH],
        masm: &mut MacroAssembler,
    ) {
        let og_guest_regs_mapping = self.guest_regs_mapping;
        let mut regs_to_save = dirty_guest_regs;
        let mut input_regs = reg_reserve!();
        for (i, &mapped_reg) in desired_guest_regs_mapping.iter().enumerate() {
            let reg = Reg::from(i as u8);
            if mapped_reg != Reg::None {
                input_regs += reg;
                if basic_block_output_regs.is_reserved(reg) {
                    regs_to_save -= reg;
                }
            }
        }

        self.save_active_guest_regs(regs_to_save, masm);

        for reg in regs_to_save {
            if desired_guest_regs_mapping[reg as usize] == Reg::None && self.guest_regs_mapping[reg as usize] != Reg::None {
                self.set_guest_reg_mapping(reg, Reg::None);
            }
        }

        for i in 0..desired_guest_regs_mapping.len() {
            let reg = Reg::from(i as u8);
            let desired_mapping = desired_guest_regs_mapping[i];
            let current_mapping = self.guest_regs_mapping[i];
            if current_mapping != Reg::None && desired_mapping != Reg::None && current_mapping != desired_mapping {
                self.swap_guest_regs(flags_update, reg, reg, desired_mapping, desired_guest_regs_mapping, masm);
            }
        }

        for (i, &desired_mapping) in desired_guest_regs_mapping.iter().enumerate() {
            let current_mapping = self.guest_regs_mapping[i];
            if desired_mapping != Reg::None && current_mapping != desired_mapping {
                Self::restore_guest_reg(Reg::from(i as u8), desired_mapping, masm);
            }
        }

        self.set_guest_regs_mappings(&og_guest_regs_mapping);
    }
}

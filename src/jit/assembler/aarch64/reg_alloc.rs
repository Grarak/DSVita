// The aarch64 guest register allocator — arm32's RegAlloc adapted to A64 registers
// (plan D5/D7): the pool is x19-x26 (callee-saved, so runtime calls emitted mid-block
// keep the mappings), the thumb low-register constraint is gone (A64 has no low/high
// encoding split), and there is no cross-basic-block relocation — the a64 backend keeps
// mappings within straight-line runs only and flushes at every control-flow
// discontinuity (branch instructions, local branch targets), so the swap machinery
// isn't needed. Guest PC and CPSR stay memory-resident.

use crate::jit::assembler::aarch64::block_asm::GUEST_REGS_PTR;
use crate::jit::assembler::{GUEST_REGS_LENGTH, GUEST_REG_POOL_SIZE};
use crate::jit::reg::{Reg, RegReserve};
use crate::logging::debug_panic;
use vixl::{A64AddrModeKind, A64MacroAssembler, A64Reg};

pub const GUEST_REG_ALLOCATIONS: [A64Reg; GUEST_REG_POOL_SIZE] = [A64Reg::X19, A64Reg::X20, A64Reg::X21, A64Reg::X22, A64Reg::X23, A64Reg::X24, A64Reg::X25, A64Reg::X26, A64Reg::X28];

const fn pool_index(reg: A64Reg) -> usize {
    if matches!(reg, A64Reg::X28) {
        8
    } else {
        reg as usize - A64Reg::X19 as usize
    }
}

pub struct A64RegAlloc {
    free_regs: u16,
    pub guest_regs_mapping: [A64Reg; GUEST_REGS_LENGTH],
    host_regs_mapping: [Reg; GUEST_REG_POOL_SIZE],
}

impl A64RegAlloc {
    pub fn new() -> Self {
        A64RegAlloc {
            free_regs: (1 << GUEST_REG_POOL_SIZE) - 1,
            guest_regs_mapping: [A64Reg::ZR; GUEST_REGS_LENGTH],
            host_regs_mapping: [Reg::None; GUEST_REG_POOL_SIZE],
        }
    }

    fn set_guest_reg_mapping(&mut self, guest_reg: Reg, mapped_reg: Option<A64Reg>) {
        let old = self.guest_regs_mapping[guest_reg as usize];
        match mapped_reg {
            Some(mapped_reg) => {
                self.guest_regs_mapping[guest_reg as usize] = mapped_reg;
                self.host_regs_mapping[pool_index(mapped_reg)] = guest_reg;
            }
            None => {
                self.guest_regs_mapping[guest_reg as usize] = A64Reg::ZR;
                if old != A64Reg::ZR {
                    self.host_regs_mapping[pool_index(old)] = Reg::None;
                }
            }
        }
    }

    fn restore_guest_reg(guest_reg: Reg, dest_reg: A64Reg, masm: &mut A64MacroAssembler) {
        masm.ldr_off(dest_reg, false, GUEST_REGS_PTR, guest_reg as i64 * 4, A64AddrModeKind::Offset);
    }

    fn spill_guest_reg(guest_reg: Reg, src_reg: A64Reg, masm: &mut A64MacroAssembler) {
        masm.str_off(src_reg, false, GUEST_REGS_PTR, guest_reg as i64 * 4, A64AddrModeKind::Offset);
    }

    fn alloc_free_guest_reg(&mut self, guest_reg: Reg, is_input: bool, masm: &mut A64MacroAssembler) -> Option<A64Reg> {
        if self.free_regs == 0 {
            return None;
        }
        let idx = self.free_regs.trailing_zeros() as usize;
        let reg = GUEST_REG_ALLOCATIONS[idx];
        self.set_guest_reg_mapping(guest_reg, Some(reg));
        self.free_regs &= !(1 << idx);
        if is_input {
            Self::restore_guest_reg(guest_reg, reg, masm);
        }
        Some(reg)
    }

    /// Allocate a host register for `guest_reg`, evicting a mapped register that isn't
    /// used by the current instruction — preferring ones that aren't live afterwards.
    /// Returns the spilled guest register, if the eviction had to write back.
    fn alloc_guest_reg(&mut self, guest_reg: Reg, is_input: bool, used_regs: RegReserve, next_live_regs: RegReserve, dirty_guest_regs: RegReserve, masm: &mut A64MacroAssembler) -> Reg {
        if self.alloc_free_guest_reg(guest_reg, is_input, masm).is_some() {
            return Reg::None;
        }

        let evict = |ra: &Self, want_dead: bool| -> Option<Reg> {
            for i in 0..GUEST_REG_POOL_SIZE {
                let owner = ra.host_regs_mapping[i];
                if owner != Reg::None && !used_regs.is_reserved(owner) && (!want_dead || !next_live_regs.is_reserved(owner)) {
                    return Some(owner);
                }
            }
            None
        };

        let victim = evict(self, true).or_else(|| evict(self, false)).unwrap_or_else(|| {
            debug_panic!("no free regs for guest mapping, used: {used_regs:?}");
        });
        let mapped_reg = self.guest_regs_mapping[victim as usize];
        let spilled = dirty_guest_regs.is_reserved(victim);
        if spilled {
            Self::spill_guest_reg(victim, mapped_reg, masm);
        }
        self.set_guest_reg_mapping(victim, None);
        self.set_guest_reg_mapping(guest_reg, Some(mapped_reg));
        if is_input {
            Self::restore_guest_reg(guest_reg, mapped_reg, masm);
        }
        if spilled {
            victim
        } else {
            Reg::None
        }
    }

    /// Allocate the instruction's registers: inputs get restored from memory when not
    /// already mapped, outputs just get a home. Returns the guest regs that were spilled
    /// by evictions (their dirty bit is gone — memory holds them now).
    pub fn alloc_guest_regs(&mut self, input_regs: RegReserve, output_regs: RegReserve, next_live_regs: RegReserve, dirty_guest_regs: RegReserve, masm: &mut A64MacroAssembler) -> RegReserve {
        let mut spilled_regs = RegReserve::new();
        let used_regs = input_regs + output_regs;
        for input_reg in input_regs {
            if self.guest_regs_mapping[input_reg as usize] == A64Reg::ZR {
                let spilled = self.alloc_guest_reg(input_reg, true, used_regs, next_live_regs, dirty_guest_regs, masm);
                if spilled != Reg::None {
                    spilled_regs += spilled;
                }
            }
        }
        for output_reg in output_regs {
            if self.guest_regs_mapping[output_reg as usize] == A64Reg::ZR {
                let spilled = self.alloc_guest_reg(output_reg, false, used_regs, next_live_regs, dirty_guest_regs, masm);
                if spilled != Reg::None {
                    spilled_regs += spilled;
                }
            }
        }
        spilled_regs
    }

    /// The host register holding `reg`. The caller must have allocated it (ZR = a bug).
    pub fn get_guest_map(&self, reg: Reg) -> A64Reg {
        let mapped = self.guest_regs_mapping[reg as usize];
        debug_assert_ne!(mapped, A64Reg::ZR, "unallocated guest reg {reg:?}");
        mapped
    }

    /// Write the dirty mapped registers back to guest memory.
    pub fn save_dirty_guest_regs(&self, dirty_guest_regs: RegReserve, masm: &mut A64MacroAssembler) {
        for guest_reg in dirty_guest_regs {
            let mapped_reg = self.guest_regs_mapping[guest_reg as usize];
            if mapped_reg != A64Reg::ZR {
                Self::spill_guest_reg(guest_reg, mapped_reg, masm);
            }
        }
    }

    /// Drop every mapping (memory must already be coherent — see save_dirty_guest_regs).
    pub fn clear(&mut self) {
        self.free_regs = (1 << GUEST_REG_POOL_SIZE) - 1;
        self.guest_regs_mapping = [A64Reg::ZR; GUEST_REGS_LENGTH];
        self.host_regs_mapping = [Reg::None; GUEST_REG_POOL_SIZE];
    }
}

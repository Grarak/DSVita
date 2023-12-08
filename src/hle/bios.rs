mod bios {
    use crate::hle::bios_lookup_table::ARM9_SWI_LOOKUP_TABLE;
    use crate::hle::registers::ThreadRegs;
    use crate::logging::debug_println;

    pub fn swi(comment: u8, regs: &mut ThreadRegs) {
        let (name, func) = &ARM9_SWI_LOOKUP_TABLE[comment as usize];
        debug_println!("Swi call {:x} {}", comment, name);
        func(regs);
    }
}

pub use bios::swi;

mod swi {
    use crate::hle::registers::ThreadRegs;

    pub fn bit_unpack(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn cpu_fast_set(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn cpu_set(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn diff_unfilt16(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn diff_unfilt8(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn divide(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn get_crc16(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn halt(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn huff_uncomp(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn interrupt_wait(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn is_debugger(regs: &mut ThreadRegs) {
        regs.gp_regs[0] = 0;
    }

    pub fn lz77_uncomp(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn runlen_uncomp(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn square_root(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn unknown(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn v_blank_intr_wait(regs: &mut ThreadRegs) {
        todo!()
    }

    pub fn wait_by_loop(regs: &mut ThreadRegs) {
        todo!()
    }
}

pub use swi::*;

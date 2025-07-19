#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ExceptionVector {
    Reset = 0x0,
    UndefinedInstruction = 0x4,
    SoftwareInterrupt = 0x8,
    PrefetchAbort = 0xC,
    DataAbort = 0x10,
    AddressExceeds26Bit = 0x14,
    NormalInterrupt = 0x18,
    FastInterrupt = 0x1C,
}

mod handler {
    use crate::core::emu::Emu;
    use crate::core::exception_handler::ExceptionVector;
    use crate::core::hle::bios;
    use crate::core::thread_regs::Cpsr;
    use crate::core::CpuType;
    use crate::logging::{debug_panic, debug_println};
    use bilge::prelude::u5;
    use std::intrinsics::likely;

    pub fn handle<const CPU: CpuType>(emu: &mut Emu, comment: u8, vector: ExceptionVector) {
        if CPU == CpuType::ARM7 || likely(emu.cp15.exception_addr != 0) {
            match vector {
                ExceptionVector::SoftwareInterrupt => bios::swi::<CPU>(comment, emu),
                ExceptionVector::NormalInterrupt => bios::interrupt::<CPU>(emu),
                _ => debug_panic!("unhandled exception vector: {vector:?}"),
            }
        } else {
            debug_println!("{CPU:?} handle exception");
            debug_assert!(vector != ExceptionVector::SoftwareInterrupt);

            const MODES: [u8; 8] = [0x13, 0x1B, 0x13, 0x17, 0x17, 0x13, 0x12, 0x11];
            let regs = CPU.thread_regs();

            let mut new_cpsr = Cpsr::from(regs.cpsr);
            new_cpsr.set_mode(u5::new(MODES[(vector as usize) >> 2]));
            new_cpsr.set_thumb(false);
            new_cpsr.set_fiq_disable(true);
            new_cpsr.set_irq_disable(true);
            emu.thread_set_cpsr(CPU, new_cpsr.into(), true);

            let regs = CPU.thread_regs();
            // Interrupt handler will subtract 4 from lr, offset this
            regs.lr = regs.pc + 4;
            regs.pc = vector as u32;
        }
    }
}

pub use handler::handle;

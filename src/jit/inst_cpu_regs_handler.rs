use crate::core::thread_regs::Cpsr;
use crate::core::CpuType::ARM9;
use crate::get_jit_asm_ptr;

pub unsafe extern "C" fn cpu_regs_halt() {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();

    // Force enable irq, this is a hack and should get properly fixed
    let mut cpsr = Cpsr::from(ARM9.thread_regs().cpsr);
    cpsr.set_irq_disable(false);
    ARM9.thread_regs().cpsr = u32::from(cpsr);

    asm.emu.cpu_halt(ARM9, 0);
}

// Per-backend emitters. Each backend owns its instruction-selection files entirely
// (the ~117 masm mnemonics the arm32 emitter uses ARE the ARM32 ISA — there is no
// meaningful shared facade at this level, see the port plan D3). The aarch64 emitter
// lands here as `aarch64/` in stage 5.
#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "arm")]
mod arm32;

macro_rules! map_fun_cpu {
    ($cpu:expr, $fun:ident) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => $fun::<{ crate::core::CpuType::ARM9 }> as *const (),
            crate::core::CpuType::ARM7 => $fun::<{ crate::core::CpuType::ARM7 }> as *const (),
        }
    }};
    ($cpu:expr, $fun:ident, $($args:tt)*) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => $fun::<{ crate::core::CpuType::ARM9 }, $($args)*> as *const (),
            crate::core::CpuType::ARM7 => $fun::<{ crate::core::CpuType::ARM7 }, $($args)*> as *const (),
        }
    }};
}

pub(crate) use map_fun_cpu;

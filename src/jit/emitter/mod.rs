mod emit;
mod emit_alu;
mod emit_branch;
mod emit_cp15;
mod emit_psr;
mod emit_swi;
mod emit_transfer;
mod thumb;

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

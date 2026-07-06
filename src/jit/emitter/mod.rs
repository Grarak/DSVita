// Per-backend emitters. Each backend owns its instruction-selection files entirely
// (the ~117 masm mnemonics the arm32 emitter uses ARE the ARM32 ISA — there is no
// meaningful shared facade at this level, see the port plan D3). The aarch64 emitter
// lands here as `aarch64/` in stage 5.
mod arm32;

pub(crate) use arm32::map_fun_cpu;

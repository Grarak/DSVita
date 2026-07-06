// The arm32 code-generation backend: vixl aarch32 macro-assembler glue, the block
// assembler / register allocator, and the raw arm/thumb instruction builders used by
// the patchers.
pub mod arm;
pub mod block_asm;
pub mod reg_alloc;
pub mod thumb;
pub mod vixl;

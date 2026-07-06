# Width audit for the A64 backend (port plan S3 wave B)

Survey of every `as u32` in `src/jit` + `src/mmap` (304 sites) and every `wrapping_*`
in `src/jit` + `src/core/memory` (59 sites), classified for stage 5. Only the *shared*
sites need work; the arm32 backend legitimately lives in a 32-bit pointer world.

## A. arm32-backend-owned — correct as-is, stays behind the seam

- `assembler/arm32/block_asm.rs`: `ldr2(reg, ptr as u32)` host-pointer bakes
  (`host_sp_ptr`, `guest_regs_ptr`, every `call()` target). The A64 emitter materializes
  pointers with its own movz/movk helpers (vixl glue `mov_imm64`).
- `jit_memory.rs` patching half: `fast_mem_mov`, `guest_inst_metadata_ptr as u32`
  (lines ~1015/1083/1105), the `SLOW_MEM_*` length budgets, `jit_entry_addr … as u32 |
  thumb` (line ~559, inside `insert()`'s entry-patch loop). All feed arm32 encoders;
  the A64 twin gets its own `patch.rs` with `SLOW_MEM_*_A64` budgets (plan D7).
- `mmap/vita.rs` `size as u32` for SCE kernel calls — vita-only by definition.

## B. shared driver/runtime sites to widen or annotate in S5

- `jit_asm.rs` block driver: `emit_validate_block_hash(guest_ptr as u32, …)` — guest_ptr
  is a HOST pointer into shm; the seam method must take `usize` and let each backend
  narrow it (arm32) or take it whole (a64). Same fn's `xxh32(slice::from_raw_parts(...))`
  is width-clean.
- `jit_memory_map.rs`: `(addr as u32) & 0x0F000000` region switches — `addr` is a guest
  pc carried as `usize`; semantically u32, safe, but worth a `guest_pc: u32` parameter
  type instead of a cast when the map grows the a64 entry table (entries already
  `HeapArrayUsize` since S1).
- `jit_memory.rs` `JitBlockMetadata` packs jit-offset pages into `u16` — offsets are
  within the 32 MB jit region, safe on both widths by construction (assert exists).
- `debug_inst_log` / gate hashing: guest-typed u32 everywhere, clean.

## C. wrapping-arithmetic audit (usize math changing meaning at 64-bit)

- `src/core/memory`: zero `wrapping_*` outside `wrapping_add_signed` branch-offset math.
  Address arithmetic uses explicit region masks on u32-typed guest addresses before any
  usize widening — no 2^32-wrap dependence found.
- `src/jit/interpreter`: 54 sites, all *guest data* arithmetic on `u32`/`u64` (ALU
  semantics, LCG in the nitrosdk HLE) — width-independent by type.
- Runtime: `return_stack_ptr.wrapping_sub(1)` (u8 ring index) and the scheduler's
  `min(a.wrapping_sub(1), b.wrapping_sub(1)).wrapping_add(1)` cycle clamp — u16 typed,
  width-independent.
- Conclusion: no silent-semantics-change candidates; the S1 worry (folded into this
  audit) is clear.

## D. width-coupled encodings (plan D4) — resolved design

`InstMemMultipleParams.op0` (u4) and `breakout_after_write`'s `mapped_reg - 4` encode
arm32 host register numbers in runtime data. Decision refined during S3: the arm32
backend KEEPS encoding its own register numbers (changing them would break emitted-code
byte identity for zero benefit); the pool-index abstraction is only the contract for
NEW backends — the a64 emitter encodes pool indices 0..7 in the same fields and its own
handlers decode them accordingly. No shared-struct change needed.

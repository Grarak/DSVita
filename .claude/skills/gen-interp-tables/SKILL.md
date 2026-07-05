---
name: gen-interp-tables
description: Regenerate the interpreter's flat dispatch tables from the disassembler lookup tables — use after adding/renaming an interpreter handler, changing the disassembler table layout, or when the interpreter table drifts from the jit decode.
---

# Regenerate interpreter dispatch tables

The interpreter's tables are GENERATED from the disassembler's lookup tables so
interpreter-decode can never disagree with jit-decode. Never edit the generated files by
hand.

| table | source of truth | generator |
|---|---|---|
| `src/jit/interpreter/thumb_table.rs` (`[ThumbInterpFn; 1024]`, indexed `op >> 6`) | `src/jit/disassembler/thumb/lookup_table_thumb.rs` | `tools/gen_thumb_table.py` |
| `src/jit/interpreter/arm_table.rs` (`[ArmInterpFn; 4096]`, indexed `((op>>16)&0xFF0)\|((op>>4)&0xF)`) | `src/jit/disassembler/lookup_table.rs` | same approach: parse the table paren-depth-aware, map each slot's disassembler fn name to the interpreter handler, unknown → `inst_fallback` |

Workflow:

1. Add the specialized handler(s) in `src/jit/interpreter/` (const generics + `paste`,
   mirroring `disassembler/delegations.rs` — no matching on the execution path; see
   DEVELOPMENT.md §2 for the design rules).
2. If the handler stays unimplemented in the interpreter, keep its name in the generator's
   FALLBACK set so the slot maps to `inst_fallback`/`inst_fallback_t` (fallback = the block
   compiles instead; correctness holds at any coverage).
3. Run the generator from the repo root: `python3 tools/gen_thumb_table.py`.
4. Build; the table references handlers by bare name, so a missing/renamed handler fails
   loudly at compile time.
5. Verify semantics against the jit with an A/B trace (inst-trace skill) if the change is
   more than mechanical.

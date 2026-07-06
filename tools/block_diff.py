#!/usr/bin/env python3
# Block-level trace diff: reduce each ilog to a per-cpu stream of BLOCK ENTRIES
# (records whose pc isn't prev_pc+step — i.e. taken-branch targets / dispatches),
# then find the first index where the two runs' block streams differ. Much coarser
# than trace_diff.py --strict: use it to localize a control-flow-level divergence
# (which block, which entry) before zooming into records.
# Usage: block_diff.py <a.ilog> <b.ilog> <cpu 0|1>     (ARM-mode pcs only for now)
import os
import sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import trace_diff as td

def block_stream(path, cpu):
    s = td.Stream(path, 100)
    prev_pc = None
    idx = 0  # inst index within cpu stream
    while True:
        r = s.head()
        if r is None:
            return
        s.pop()
        if r[0] != cpu:
            continue
        idx += 1
        pc = r[1]
        if prev_pc is None or pc != prev_pc + 4:  # ARM only (no thumb blocks compile)
            yield (pc, idx)
        prev_pc = pc

def main(a, b, cpu):
    ga, gb = block_stream(a, cpu), block_stream(b, cpu)
    n = 0
    ctx = []
    while True:
        ea, eb = next(ga, None), next(gb, None)
        if ea is None or eb is None:
            print(f"cpu{cpu}: streams ended at block #{n} (a={ea} b={eb}) — no entry diff")
            return
        n += 1
        if ea[0] != eb[0]:
            print(f"cpu{cpu}: FIRST BLOCK-ENTRY DIFF at block #{n}")
            print(f"  last {len(ctx)} common entries: " + " ".join(hex(p) for p, _ in ctx))
            print(f"  A: pc={ea[0]:x} (inst idx {ea[1]})")
            print(f"  B: pc={eb[0]:x} (inst idx {eb[1]})")
            return
        if ea[1] != eb[1]:
            print(f"cpu{cpu}: same entry pc {ea[0]:x} but INST-INDEX SKEW at block #{n}: A idx {ea[1]} vs B idx {eb[1]} (delta {eb[1]-ea[1]})")
            print(f"  preceding entries: " + " ".join(hex(p) for p, _ in ctx[-8:]))
            return
        ctx.append(ea)
        if len(ctx) > 16:
            ctx.pop(0)

if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2], int(sys.argv[3]))

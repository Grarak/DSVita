#!/usr/bin/env python3
# Self-calibrating compare for the armv7 byte-identity gate (see armv7_gate.sh).
#
# Blocks that bake runtime heap pointers (the HLE bios / os-irq / TWL substitutions) hash
# differently on every run, so plain stream equality can never pass. Two baseline runs of
# the SAME build calibrate which (cpu, pc, thumb) blocks are run-stable; a candidate build
# must then (a) produce identical hashes on every stable block, and (b) agree on length
# for every block present in both. First occurrence per key wins (recompiles after
# invalidation can legitimately change content mid-run).
import sys


def load(path):
    first = {}
    for line in open(path):
        cpu, pc, thumb, ln, h = line.split()
        key = (cpu, pc, thumb)
        if key not in first:
            first[key] = (int(ln), h)
    return first


def main():
    if len(sys.argv) != 5:
        print("usage: armv7_gate_compare.py <name> <base1.blocks> <base2.blocks> <candidate.blocks>", file=sys.stderr)
        return 2
    name, b1p, b2p, cp = sys.argv[1:]
    b1, b2, cand = load(b1p), load(b2p), load(cp)

    stable = {k: v for k, v in b1.items() if b2.get(k) == v}
    common_stable = [k for k in stable if k in cand]
    if len(common_stable) < 150:
        print(f"{name}: FAIL — only {len(common_stable)} common stable blocks (need 150+; run longer)")
        return 1

    hash_bad = [k for k in common_stable if cand[k] != stable[k]]
    len_bad = [k for k in b1 if k in cand and cand[k][0] != b1[k][0] and k not in stable]

    unstable = len(b1) - len(stable)
    if hash_bad or len_bad:
        print(f"{name}: FAIL — {len(hash_bad)} hash mismatches on stable blocks, {len(len_bad)} length mismatches on unstable blocks")
        for k in (hash_bad + len_bad)[:5]:
            print(f"  cpu{k[0]} pc={k[1]} thumb={k[2]}: base={b1[k]} cand={cand[k]}")
        return 1
    print(f"{name}: OK — {len(common_stable)} stable blocks byte-identical ({unstable} pointer-baking blocks length-checked only)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

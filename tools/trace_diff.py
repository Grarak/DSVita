#!/usr/bin/env python3
# Streaming binary ilog differ. Finds the first divergence between two instruction traces
# (as produced by dsvita --inst-log / --inst-log-lazy with DEBUG_LOG=true).
#
# Usage: trace_diff.py <a.ilog> <b.ilog> [window=200000]
#
# Divergence model (per the trace-diff parity rules in DEVELOPMENT.md):
#   - Both traces omit taken branches (jit doesn't log them; interp logs only Continue).
#   - Spin/io-poll loops legitimately differ in iteration count between engines (timing skew).
#   - jit logs same-block `bl` records that the interp omits -> small 1-record skews.
#   - FIRST same-(cpu,pc)-different-fields = real bug. Before that, all (cpu,pc) mismatches
#     are resyncable as iteration-count skew.
#
# Resync: when heads disagree on (cpu,pc), find the first (cpu,pc) that appears in BOTH
# streams' next W records and advance both to it. This handles three benign misalignments:
#   1. spin/io-poll iteration-count skew (loop exit pc common to both),
#   2. same-block bl parity gap (jit logs the bl, interp doesn't -> post-bl pc common),
#   3. HLE gaps (jit HLE-replaces a cold SDK function -> body unlogged; interp interprets it
#      -> body logged; the function return point pc is common to both).
# If no common (cpu,pc) within W in both, declare a real control-flow divergence and stop.
#
# Output: the first divergence (same-pc-diff-regs, or unresyncable control-flow split), with
# the offending register delta and the 1-based pair index. Pair index ~= number of lockstep
# matches before the divergence, useful for cross-referencing a decoded text dump.

import struct
import sys

RECORD_SIZE = 80  # InstLogRecord: regs[15]+pc+cpsr+spsr+opcode+cpu, repr(C), 4-byte aligned
TAG_INST = 0
TAG_TEXT = 1
TAG_INST_DELTA = 2  # delta vs the same cpu's previous record; see debug_inst_log.rs

FLAG_CPU7 = 1 << 0
FLAG_PC_SEQ = 1 << 1
FLAG_OPCODE_SAME = 1 << 2
FLAG_SPSR_SAME = 1 << 3
FLAG_CPSR_SAME = 1 << 4

# Optional cpu filter: if set (via --cpu N), read_inst skips records from other cpus.
# Used when diffing -e 0 (LLE, has arm7 records) vs -e 2 (HLE, arm7 absent) — filter to
# cpu=0 (ARM9) only so the arm7 records in the LLE trace don't desync the lockstep walk.
CPU_FILTER = None


def read_inst(f, prev):
    """Read one inst record, skipping any interleaved text records. None at EOF.
    `prev` is the caller-owned per-cpu delta state ([rec_or_None, rec_or_None]); it is
    updated on EVERY record, including ones skipped by CPU_FILTER."""
    while True:
        tag = f.read(1)
        if not tag:
            return None
        t = tag[0]
        if t == TAG_INST:
            buf = f.read(RECORD_SIZE)
            if len(buf) < RECORD_SIZE:
                return None
            regs = struct.unpack("<15I", buf[0:60])
            pc, cpsr, spsr, opcode = struct.unpack("<4I", buf[60:76])
            cpu = buf[76]
            rec = (cpu, pc, opcode, cpsr, spsr, regs)
            if cpu < 2:
                prev[cpu] = rec
            if CPU_FILTER is not None and cpu != CPU_FILTER:
                continue
            return rec
        elif t == TAG_INST_DELTA:
            head = f.read(3)
            if len(head) < 3:
                return None
            flags = head[0]
            mask = head[1] | (head[2] << 8)
            cpu = flags & FLAG_CPU7
            p = prev[cpu]
            if p is None:
                return None  # delta before keyframe — corrupt
            nwords = (
                (0 if flags & FLAG_PC_SEQ else 1)
                + (0 if flags & FLAG_CPSR_SAME else 1)
                + (0 if flags & FLAG_SPSR_SAME else 1)
                + (0 if flags & FLAG_OPCODE_SAME else 1)
                + bin(mask).count("1")
            )
            data = f.read(4 * nwords)
            if len(data) < 4 * nwords:
                return None
            words = struct.unpack(f"<{nwords}I", data) if nwords else ()
            w = 0
            _, ppc, popcode, pcpsr, pspsr, pregs = p
            if flags & FLAG_PC_SEQ:
                pc = None  # resolved after cpsr
            else:
                pc = words[w]
                w += 1
            if flags & FLAG_CPSR_SAME:
                cpsr = pcpsr
            else:
                cpsr = words[w]
                w += 1
            if flags & FLAG_SPSR_SAME:
                spsr = pspsr
            else:
                spsr = words[w]
                w += 1
            if flags & FLAG_OPCODE_SAME:
                opcode = popcode
            else:
                opcode = words[w]
                w += 1
            if mask:
                regs = list(pregs)
                for i in range(15):
                    if mask & (1 << i):
                        regs[i] = words[w]
                        w += 1
                regs = tuple(regs)
            else:
                regs = pregs
            if pc is None:
                pc = (ppc + (2 if cpsr & 0x20 else 4)) & 0xFFFFFFFF
            rec = (cpu, pc, opcode, cpsr, spsr, regs)
            prev[cpu] = rec
            if CPU_FILTER is not None and cpu != CPU_FILTER:
                continue
            return rec
        elif t == TAG_TEXT:
            (ln,) = struct.unpack("<I", f.read(4))
            f.read(ln)
            f.read(1)  # newline flag
        else:
            return None  # corrupt tail from killed process — treat as EOF


class Stream:
    """Sliding-window stream over inst records with bounded lookahead buffer."""

    __slots__ = ("f", "W", "buf", "hi", "read_count", "prev")

    def __init__(self, path, W):
        self.f = open(path, "rb")
        self.W = W
        self.buf = []
        self.hi = 0
        self.read_count = 0
        self.prev = [None, None]

    def _ensure(self, n):
        while len(self.buf) - self.hi < n:
            r = read_inst(self.f, self.prev)
            if r is None:
                return False
            self.buf.append(r)
            self.read_count += 1
        return True

    def head(self):
        if self.hi >= len(self.buf):
            if not self._ensure(1):
                return None
        return self.buf[self.hi]

    def pop(self):
        self.hi += 1
        if self.hi >= 4096:
            del self.buf[: self.hi]
            self.hi = 0

    def peek(self, o):
        """Record at lookahead offset o (1-based beyond head). None if unavailable."""
        if not self._ensure(o + 1):
            return None
        return self.buf[self.hi + o]

    def key_at(self, o):
        """(cpu,pc) at lookahead offset o, or None if unavailable."""
        if not self._ensure(o + 1):
            return None
        r = self.buf[self.hi + o]
        return (r[0], r[1])

    def skip(self, n):
        self.hi += n
        if self.hi >= 4096:
            del self.buf[: self.hi]
            self.hi = 0


def reg_name(i):
    names = ["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr"]
    return names[i]


def main(pathA, pathB, W):
    A = Stream(pathA, W)
    B = Stream(pathB, W)
    n = 0
    resyncs = 0
    skipped_a = 0
    skipped_b = 0
    progress_every = 2_000_000
    max_logged = 200000
    div_count = 0
    logged_divs = 0
    W_small = 2000  # fast first-pass window; full W only used when small fails (HLE gaps)
    # Tally which fields differ, to spot the dominant benign pattern (HLE copy side effects).
    field_tally = {"opcode": 0, "cpsr": 0, "spsr": 0}
    reg_tally = [0] * 15
    last_div_pc = -1
    last_div_n = -1

    while True:
        a = A.head()
        b = B.head()
        if a is None or b is None:
            print(f"EOF: a_done={a is None} b_done={b is None}; matched={n} resyncs={resyncs} "
                  f"skipped_a={skipped_a} skipped_b={skipped_b} divs={div_count}")
            break

        ka = (a[0], a[1])
        kb = (b[0], b[1])

        if ka == kb:
            n += 1
            # a/b: (cpu, pc, opcode, cpsr, spsr, regs)
            if a[2] != b[2] or a[3] != b[3] or a[4] != b[4] or a[5] != b[5]:
                div_count += 1
                cpu = a[0]
                pc = a[1]
                if a[2] != b[2]:
                    field_tally["opcode"] += 1
                if a[3] != b[3]:
                    field_tally["cpsr"] += 1
                if a[4] != b[4]:
                    field_tally["spsr"] += 1
                diff_regs = []
                interesting = a[2] != b[2]  # opcode diff is always interesting
                for i in range(15):
                    if a[5][i] != b[5][i]:
                        reg_tally[i] += 1
                        diff_regs.append(f"{reg_name(i)}={a[5][i]:#x}/{b[5][i]:#x}")
                        if i != 12:  # r12-only diffs are the benign HLE-copy pattern
                            interesting = True
                # Only log "interesting" divergences (not r12/cpsr/spsr-only). Dedup by pc.
                if interesting and logged_divs < max_logged and (pc != last_div_pc or n - last_div_n > 4096):
                    marker = " [OPCODE-DIFF]" if a[2] != b[2] else (
                        " [GPR-LO-DIFF]" if (a[5][0] != b[5][0] or a[5][1] != b[5][1]
                                             or a[5][2] != b[5][2] or a[5][3] != b[5][3]) else "")
                    print(f"#{n} cpu{cpu} pc={pc:#x} cpsr={a[3]:#x}/{b[3]:#x}{marker} "
                          f"{' '.join(diff_regs)}")
                    last_div_pc = pc
                    last_div_n = n
                    logged_divs += 1
                    if logged_divs == max_logged:
                        print(f"... hit max_logged={max_logged}, further divergences counted but not logged ...")
                A.pop()
                B.pop()
            else:
                A.pop()
                B.pop()
                if n % progress_every == 0:
                    print(f"... matched {n} pairs (resyncs={resyncs} divs={div_count})")
        else:
            # (cpu,pc) mismatch: find the (cpu,pc) common to both streams with the
            # SMALLEST total skip, and advance both to it. Handles iteration-count skew,
            # same-block bl parity gaps, HLE gaps, and one-side-already-at-target cases.
            # Two-phase: try a small window first (fast, covers the common tiny skews),
            # fall back to the full W only if the small window fails (HLE gaps ~8K records).
            def find_resync(win):
                A._ensure(win)
                B._ensure(win)
                am = {}
                a_avail = len(A.buf) - A.hi
                for o in range(0, min(win + 1, a_avail)):
                    r = A.buf[A.hi + o]
                    k = (r[0], r[1])
                    if k not in am:
                        am[k] = o
                boa = -1
                bob = -1
                bt = 1 << 60
                b_avail = len(B.buf) - B.hi
                for o in range(0, min(win + 1, b_avail)):
                    r = B.buf[B.hi + o]
                    oa = am.get((r[0], r[1]), -1)
                    if oa >= 0:
                        total = oa + o
                        if total < bt:
                            bt = total
                            boa = oa
                            bob = o
                            if total <= 1:
                                break
                return boa, bob

            best_oa, best_ob = find_resync(W_small)
            if best_oa < 0 or best_ob < 0:
                best_oa, best_ob = find_resync(W)
            if best_oa >= 0 and best_ob >= 0 and (best_oa > 0 or best_ob > 0):
                if best_oa + best_ob > 500 or resyncs < 50:
                    print(f"  resync#{resyncs} @pair#{n}: A.pc={a[1]:#x} B.pc={b[1]:#x} "
                          f"skip_a={best_oa} skip_b={best_ob}")
                A.skip(best_oa)
                B.skip(best_ob)
                skipped_a += best_oa
                skipped_b += best_ob
                resyncs += 1
            else:
                cpu_a, pc_a = a[0], a[1]
                cpu_b, pc_b = b[0], b[1]
                print(f"*** UNRESOLVABLE CONTROL-FLOW SPLIT at pair #{n} "
                      f"(resyncs={resyncs} skipped_a={skipped_a} skipped_b={skipped_b} divs={div_count}):")
                print(f"  no common (cpu,pc) within W={W} (best_oa={best_oa} best_ob={best_ob})")
                print(f"  A ({pathA}): cpu={cpu_a} pc={pc_a:#x} opcode={a[2]:#x} cpsr={a[3]:#x}")
                print(f"       regs={[f'{reg_name(i)}={a[5][i]:#x}' for i in range(15)]}")
                print(f"  B ({pathB}): cpu={cpu_b} pc={pc_b:#x} opcode={b[2]:#x} cpsr={b[3]:#x}")
                print(f"       regs={[f'{reg_name(i)}={b[5][i]:#x}' for i in range(15)]}")
                print(f"  A read_count={A.read_count}  B read_count={B.read_count}")
                print("  *** THIS is the real divergence (control flow permanently splits) ***")
                break

    print("\n=== SUMMARY ===")
    print(f"matched pairs: {n}")
    print(f"resyncs: {resyncs}  skipped_a: {skipped_a}  skipped_b: {skipped_b}")
    print(f"same-pc-different-fields divergences: {div_count}")
    print(f"field tally: {field_tally}")
    print("reg diff tally (how many divergences touched each reg):")
    for i in range(15):
        if reg_tally[i]:
            print(f"  {reg_name(i):>4}: {reg_tally[i]}")
    print(f"A read_count={A.read_count}  B read_count={B.read_count}")


def fmt_rec(r):
    regs = " ".join(f"{reg_name(i)}={r[5][i]:#x}" for i in range(15))
    return f"cpu{r[0]} pc={r[1]:#x} op={r[2]:#x} cpsr={r[3]:#x} spsr={r[4]:#x} {regs}"


def strict_main(pathA, pathB):
    """Same-engine mode: the streams must match record-for-record, no resync, no tolerated
    field diffs. First difference of any kind is reported with context and exits 1; a clean
    run (EOF on either side with everything before it identical) exits 0. Interpreter-vs-
    interpreter (and later jit-vs-jit) pairs must pass this."""
    A = Stream(pathA, 16)
    B = Stream(pathB, 16)
    n = 0
    progress_every = 10_000_000
    while True:
        a = A.head()
        b = B.head()
        if a is None or b is None:
            side = "A" if a is None else "B"
            print(f"STRICT PASS to EOF({side}): {n} records identical "
                  f"(A read={A.read_count} B read={B.read_count})")
            return 0
        if a != b:
            print(f"*** STRICT DIVERGENCE at record #{n} ***")
            print(f"  A ({pathA}):")
            for o in range(0, 4):
                r = A.peek(o) if o else a
                if r is not None:
                    print(f"    +{o} {fmt_rec(r)}")
            print(f"  B ({pathB}):")
            for o in range(0, 4):
                r = B.peek(o) if o else b
                if r is not None:
                    print(f"    +{o} {fmt_rec(r)}")
            if (a[0], a[1]) == (b[0], b[1]):
                diffs = []
                for name, ia in (("opcode", 2), ("cpsr", 3), ("spsr", 4)):
                    if a[ia] != b[ia]:
                        diffs.append(f"{name}={a[ia]:#x}/{b[ia]:#x}")
                diffs += [f"{reg_name(i)}={a[5][i]:#x}/{b[5][i]:#x}" for i in range(15) if a[5][i] != b[5][i]]
                print(f"  same (cpu,pc), fields differ: {' '.join(diffs)}")
            else:
                print("  control flow split (different cpu/pc)")
            return 1
        n += 1
        A.pop()
        B.pop()
        if n % progress_every == 0:
            print(f"... {n} records identical")


if __name__ == "__main__":
    args = sys.argv[1:]
    if len(args) < 2:
        print("Usage: trace_diff.py <a.ilog> <b.ilog> [window=100000] [--cpu N] [--strict]", file=sys.stderr)
        sys.exit(2)
    pathA = args[0]
    pathB = args[1]
    W = 100000
    strict = False
    i = 2
    while i < len(args):
        a = args[i]
        if a == "--cpu" and i + 1 < len(args):
            CPU_FILTER = int(args[i + 1])
            i += 2
        elif a.startswith("--cpu="):
            CPU_FILTER = int(a.split("=")[1])
            i += 1
        elif a == "--strict":
            strict = True
            i += 1
        else:
            W = int(a)
            i += 1
    if strict:
        sys.exit(strict_main(pathA, pathB))
    main(pathA, pathB, W)

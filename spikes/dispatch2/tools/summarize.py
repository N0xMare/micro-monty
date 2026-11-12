#!/usr/bin/env python3
"""Markdown tables from results/: instructions, wall time, ratios, stability.

usage: tools/summarize.py [results dir] > results/summary.md
"""
import csv
import os
import statistics
import sys

d = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "results")
PROGS = ["fib", "closure", "callfn", "loopfn", "loop", "dictint"]
ENGS = ["dispatch2", "qjs", "mm"]
FIB_CALLS = 635_621


def load_cg(name):
    out = {}
    for row in csv.reader(open(os.path.join(d, name))):
        if row and not row[0].startswith("#"):
            out[(row[0], row[1])] = int(row[2])
    return out


def load_wall(name):
    out = {}
    for line in open(os.path.join(d, name)):
        if line.startswith("# raw"):
            break
        f = line.strip().split(",")
        if len(f) == 5 and f[0] in ENGS:
            out[(f[0], f[1])] = float(f[2])
    return out


cg = load_cg("callgrind.csv")
walls = [load_wall(f) for f in sorted(os.listdir(d)) if f.startswith("wall_run")]
M = lambda n: f"{n / 1e6:,.1f}M"

print("## Instructions, whole process (callgrind) [M]\n")
print("| program | dispatch2 | qjs | mm | dispatch2 / qjs | mm / qjs | dispatch2 / mm |")
print("|---|---|---|---|---|---|---|")
for p in PROGS + ["empty"]:
    s, q, m = cg[("dispatch2", p)], cg[("qjs", p)], cg[("mm", p)]
    print(f"| {p} | {M(s)} | {M(q)} | {M(m)} | {s / q:.2f} | {m / q:.2f} | {s / m:.2f} |")

print("\n## Instructions net of each engine's empty program [M]\n")
print("| program | dispatch2 | qjs | mm | dispatch2 / qjs | mm / qjs |")
print("|---|---|---|---|---|---|")
net = {}
for p in PROGS:
    for e in ENGS:
        net[(e, p)] = cg[(e, p)] - cg[(e, "empty")]
    s, q, m = (net[(e, p)] for e in ENGS)
    print(f"| {p} | {M(s)} | {M(q)} | {M(m)} | {s / q:.2f} | {m / q:.2f} |")

print("\n## Instructions per call [M]\n")
print("| measure | dispatch2 | qjs | mm |")
print("|---|---|---|---|")
row = [(net[(e, "callfn")] - net[(e, "loopfn")]) / 1e6 for e in ENGS]
print("| call statement `total = inc(total)` minus `total = total + 1` (callfn − loopfn) / 1M | " + " | ".join(f"{x:.0f}" for x in row) + " |")
row = [net[(e, "fib")] / FIB_CALLS for e in ENGS]
print(f"| fib, all instructions per call ({FIB_CALLS:,} calls) | " + " | ".join(f"{x:.0f}" for x in row) + " |")
row = [net[(e, "closure")] / 1e6 for e in ENGS]
print("| closure, all instructions per iteration (1 call each) | " + " | ".join(f"{x:.0f}" for x in row) + " |")

print("\n## Wall time, ms, whole process: median of 21 pinned interleaved rounds [M]\n")
hdr = " | ".join(f"{e} run{i + 1}" for e in ENGS for i in range(len(walls)))
print(f"| program | {hdr} | dispatch2 / qjs | mm / qjs |")
print("|---" * (1 + len(ENGS) * len(walls) + 2) + "|")
for p in PROGS + ["empty"]:
    cells = [f"{w[(e, p)]:.1f}" for e in ENGS for w in walls]
    rs = statistics.mean(w[("dispatch2", p)] / w[("qjs", p)] for w in walls)
    rm = statistics.mean(w[("mm", p)] / w[("qjs", p)] for w in walls)
    print(f"| {p} | " + " | ".join(cells) + f" | {rs:.2f} | {rm:.2f} |")

print("\n## Wall time net of each engine's empty program, ms (mean of the runs' medians) [M]\n")
print("| program | dispatch2 | qjs | mm | dispatch2 / qjs | mm / qjs | projected: dispatch2 kernel + mm startup, / qjs whole process [E] |")
print("|---|---|---|---|---|---|---|")
for p in PROGS:
    k = {e: statistics.mean(w[(e, p)] - w[(e, "empty")] for w in walls) for e in ENGS}
    whole_q = statistics.mean(w[("qjs", p)] for w in walls)
    mm_empty = statistics.mean(w[("mm", "empty")] for w in walls)
    proj = (k["dispatch2"] + mm_empty) / whole_q
    print(f"| {p} | {k['dispatch2']:.1f} | {k['qjs']:.1f} | {k['mm']:.1f} | {k['dispatch2'] / k['qjs']:.2f} | {k['mm'] / k['qjs']:.2f} | {proj:.2f} |")

for name, title in [("stability_v1.csv", "first loop"), ("stability.csv", "final loop")]:
    st = load_cg(name)
    print(f"\n## Unrelated edits, {title}: instruction change vs no edit [M]\n")
    feats = ["perturb_arm", "perturb_order", "perturb_field", "perturb_cold", "perturb_slow"]
    print("| program | " + " | ".join(feats) + " |")
    print("|---" * (len(feats) + 1) + "|")
    for p in PROGS:
        base = st[("none", p)]
        print(f"| {p} | " + " | ".join(f"{(st[(f, p)] - base) / base * 100:+.2f}%" for f in feats) + " |")

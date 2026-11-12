#!/usr/bin/env python3
"""Turn the raw files in results/ into the Markdown tables used by
docs/spikes/dispatch.md. Usage: python3 tools/summarize.py results"""

import csv
import io
import os
import sys

KERNELS = ["fib", "loop", "closure", "attr", "method", "list", "dict"]
VARIANTS = [f"{d}/{m}" for d in ["stack", "accumulator", "register3"] for m in ["match", "fntable"]]
DESIGNS = ["stack", "accumulator", "register3"]


def csv_block(path, header):
    """Rows of the CSV block that starts with `header` and ends at a blank line."""
    lines = open(path).read().splitlines()
    start = lines.index(header)
    block = []
    for line in lines[start:]:
        if not line.strip():
            break
        block.append(line)
    return list(csv.DictReader(io.StringIO("\n".join(block))))


def timings(path, header, key):
    return {(r[key], r["kernel"]): (float(r["median_ms"]), float(r["min_ms"])) for r in csv_block(path, header)}


def main():
    d = sys.argv[1]
    p = lambda f: os.path.join(d, f)
    bh = "variant,kernel,reps,min_ms,median_ms,max_ms"
    eh = "engine,kernel,reps,min_ms,median_ms,max_ms,result,expected,ok"
    checked = timings(p("bench_checked.txt"), bh, "variant")
    unchecked = timings(p("bench_unchecked.txt"), bh, "variant")
    ref = {}
    ref.update(timings(p("cpython.txt"), eh, "engine"))
    ref.update(timings(p("qjs.txt"), eh, "engine"))

    out = []
    w = out.append
    w("## Wall time, ms: median / min [M]\n")
    cols = VARIANTS + ["cpython", "qjs"]
    w("| kernel | " + " | ".join(cols) + " |")
    w("|---" * (len(cols) + 1) + "|")
    for k in KERNELS:
        cells = []
        for c in cols:
            med, mn = (checked if c in VARIANTS else ref)[(c, k)]
            cells.append(f"{med:.1f} / {mn:.1f}")
        w(f"| {k} | " + " | ".join(cells) + " |")

    w("\n## Median relative to stack/match (lower is faster) [M]\n")
    w("| kernel | " + " | ".join(VARIANTS + ["cpython", "qjs"]) + " |")
    w("|---" * (len(VARIANTS) + 3) + "|")
    for k in KERNELS:
        base = checked[("stack/match", k)][0]
        cells = [f"{checked[(v, k)][0] / base:.2f}" for v in VARIANTS]
        cells += [f"{ref[(e, k)][0] / base:.2f}" for e in ["cpython", "qjs"]]
        w(f"| {k} | " + " | ".join(cells) + " |")

    static = {(r["design"], r["kernel"]): r for r in csv_block(p("stats.txt"), "design,kernel,bytecode_bytes,static_instructions")}
    dyn = {(r["variant"], r["kernel"]): r for r in csv_block(p("stats.txt"), "variant,kernel,dispatched,ticks,result_ok")}
    w("\n## Instructions dispatched per run, bytecode bytes, static instructions [M]\n")
    w("| kernel | " + " | ".join(f"{x} dispatched" for x in DESIGNS) + " | "
      + " | ".join(f"{x} bytes" for x in DESIGNS) + " | " + " | ".join(f"{x} static" for x in DESIGNS) + " |")
    w("|---" * 10 + "|")
    for k in KERNELS:
        cells = []
        for x in DESIGNS:
            a, b = dyn[(f"{x}/match", k)]["dispatched"], dyn[(f"{x}/fntable", k)]["dispatched"]
            assert a == b, (x, k, a, b)
            cells.append(f"{int(a):,}")
        cells += [static[(x, k)]["bytecode_bytes"] for x in DESIGNS]
        cells += [static[(x, k)]["static_instructions"] for x in DESIGNS]
        w(f"| {k} | " + " | ".join(cells) + " |")
    w("\nDispatched relative to stack:\n")
    w("| kernel | accumulator | register3 |")
    w("|---|---|---|")
    for k in KERNELS:
        s = int(dyn[("stack/match", k)]["dispatched"])
        w(f"| {k} | " + " | ".join(f"{int(dyn[(f'{x}/match', k)]['dispatched']) / s:.2f}" for x in DESIGNS[1:]) + " |")

    w("\n## Median ns per dispatched instruction [M]\n")
    w("| kernel | " + " | ".join(VARIANTS) + " |")
    w("|---" * (len(VARIANTS) + 1) + "|")
    for k in KERNELS:
        cells = []
        for v in VARIANTS:
            n = int(dyn[(v, k)]["dispatched"])
            cells.append(f"{checked[(v, k)][0] * 1e6 / n:.2f}")
        w(f"| {k} | " + " | ".join(cells) + " |")

    w("\n## Handler code size, bytes [M]\n")
    w("| variant | bytes | functions | detail |")
    w("|---|---|---|---|")
    for r in csv.DictReader(open(p("handler_sizes.csv"))):
        detail = r["detail"] if not r["variant"].startswith("shared") else "slow paths, dict/list helpers, natives (used by all six)"
        w(f"| {r['variant']} | {int(r['bytes']):,} | {r['functions']} | {detail} |")

    w("\n## Bounds checks: checked vs `--features unchecked`, median ms [M]\n")
    w("| kernel | " + " | ".join(VARIANTS) + " |")
    w("|---" * (len(VARIANTS) + 1) + "|")
    for k in KERNELS:
        cells = []
        for v in VARIANTS:
            c, u = checked[(v, k)][0], unchecked[(v, k)][0]
            cells.append(f"{c:.1f} → {u:.1f} ({(u - c) / c * 100:+.0f}%)")
        w(f"| {k} | " + " | ".join(cells) + " |")
    run2 = p("bench_checked_run2.txt")
    if os.path.exists(run2):
        second = timings(run2, bh, "variant")
        w("\n## Run-to-run: second process, same binary, median change vs the first [M]\n")
        w("| kernel | " + " | ".join(VARIANTS) + " |")
        w("|---" * (len(VARIANTS) + 1) + "|")
        for k in KERNELS:
            cells = []
            for v in VARIANTS:
                a, b = checked[(v, k)][0], second[(v, k)][0]
                cells.append(f"{(b - a) / a * 100:+.0f}%")
            w(f"| {k} | " + " | ".join(cells) + " |")
    print("\n".join(out))


if __name__ == "__main__":
    main()

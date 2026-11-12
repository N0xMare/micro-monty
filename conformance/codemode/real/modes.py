#!/usr/bin/env python3
"""Compares the worker modes on real sessions: Monty child and mm child
(results/phase1) against mm native, wasm and native + cache in process
(results/phase2, the same prompts and models).

    python3 modes.py results/phase1 results/phase2 RAW_PHASE2

Memory differs by mode: for a child it is the worker's peak RSS; in process
it is the growth of plak's own RSS over the session (the probe before the
first cell to the highest peak after any cell), from the raw traces.
"""

import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path

ORDER = ["monty", "mm", "native", "wasm", "cache"]


def load(directory):
    cells = [json.loads(l) for l in (Path(directory) / "cells.jsonl").open()]
    sessions = [json.loads(l) for l in (Path(directory) / "sessions.jsonl").open()]
    return cells, sessions


def growth_kib(raw):
    """(arm, prompt) -> plak's RSS growth over an in-process session, KiB."""
    out = {}
    for run_file in Path(raw).glob("*/*/run.json"):
        run = json.loads(run_file.read_text())
        first, peak = None, 0
        for trace in run_file.parent.glob("trace-*.jsonl"):
            for line in trace.open():
                item = json.loads(line)
                if item.get("d") == "p":
                    first = item["rss_kb"] if first is None else first
                    peak = max(peak, item["hwm_kb"])
        if first is not None:
            out[(run["arm"], run["prompt"])] = peak - first
    return out


def med(values):
    return statistics.median(values) if values else None


def p90(values):
    """As census.py's."""
    values = sorted(values)
    return values[int(0.9 * (len(values) - 1))] if values else None


def main():
    c1, s1 = load(sys.argv[1])
    c2, s2 = load(sys.argv[2])
    growth = growth_kib(sys.argv[3])
    cells, sessions = c1 + c2, s1 + s2
    by_arm = defaultdict(list)
    for cell in cells:
        by_arm[cell["arm"]].append(cell)
    print("| family | composition | mode | sessions | answered | cells | failed | median engine ms "
          "| p90 engine ms | median host call ms | memory KiB (median per session) |")
    print("|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    groups = defaultdict(list)
    for s in sessions:
        groups[(s["account"], s["composition"], s["worker"])].append(s)
    for (family, composition, mode), runs in sorted(
            groups.items(), key=lambda x: (x[0][0], x[0][1], ORDER.index(x[0][2]))):
        arm = runs[0]["arm"]
        cs = by_arm[arm]
        engine = [c["engine_ms"] for c in cs if c["engine_ms"] is not None]
        waits = [h["ms"] for c in cs for h in c["host_calls"] or []]
        if mode in ("monty", "mm"):
            memory = med([s["worker_peak_rss_kb"] for s in runs])
            memory = f"{memory:,.0f} (worker peak)"
        else:
            memory = med([growth[(s["arm"], s["prompt"])] for s in runs
                          if (s["arm"], s["prompt"]) in growth])
            memory = f"{memory:,.0f} (plak growth)"
        answered = sum(1 for s in runs if s["exit"] == 0 and s["answer_chars"] > 0)
        print(f"| {family} | {composition} | {mode} | {len(runs)} | {answered} | {len(cs)} | "
              f"{sum(c['outcome'] != 'completed' for c in cs)} | {med(engine):.2f} | "
              f"{p90(engine):.2f} | {med(waits):.1f} | {memory} |")


main()

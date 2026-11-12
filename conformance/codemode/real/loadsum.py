#!/usr/bin/env python3
"""Summarizes `inproc_load` summary lines: medians over repetitions.

    python3 loadsum.py results/load/sweep.jsonl

One row per (sessions, mode). Times are medians over the sessions of a run,
then over runs; `rss_per_session_kib` is (process RSS + child RSS - the
baseline before the burst) / sessions; the last column lists each run's
total RSS (process plus children) in KiB.
"""

import collections
import json
import statistics
import sys

KEYS = ["spawn_ms_median", "import_cell_ms_median", "calls_cell_ms_median",
        "per_host_call_us_median", "replay_cell_ms_median", "rss_per_session_kib", "failures"]
ORDER = ["monty", "mm", "native", "wasm", "native-cache"]

rows = [json.loads(line) for line in open(sys.argv[1])]
groups = collections.defaultdict(list)
for row in rows:
    groups[(row["sessions"], row["mode"])].append(row)
print("| N | mode | " + " | ".join(KEYS) + " | total RSS KiB per run |")
print("|---" * (len(KEYS) + 3) + "|")
for (n, mode), runs in sorted(groups.items(), key=lambda x: (x[0][0], ORDER.index(x[0][1]))):
    values = []
    for key in KEYS:
        v = [r[key] for r in runs if r[key] is not None]
        values.append("%.2f" % statistics.median(v) if v else "-")
    totals = [r["rss_kib"] + r["children_rss_kib"] for r in runs]
    print(f"| {n} | {mode} | " + " | ".join(values) + f" | {totals} |")

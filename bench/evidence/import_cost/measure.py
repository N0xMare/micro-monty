"""Per-module import cost: steady Ir, wall time, RSS.

    python3.14 -I bench/evidence/import_cost/measure.py OUT.json [--mods a,b] [--reps N] [--no-ir]

Needs `imp` (built into target/import-cost, or `$IMP`) and valgrind. For each
entry of MODS (a module, or a `+`-joined set imported in one cell), steady
Ir = (Ir(imp ir M 11) - Ir(imp ir M 1)) / 10 minus the same for no import;
wall = median of REPS under `taskset -c 2`, minus no import's median;
RSS = (after - before) / 40 sessions, each holding its imports.
`--no-wall` measures Ir only. `$IMP` picks another build of `imp`;
`IMP_CACHE=0` turns the decoded-module cache off (steady Ir then includes
each session's decode).
"""
import argparse
import concurrent.futures
import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
IMP = Path(os.environ.get("IMP", ROOT / "target/import-cost/release/imp"))
MODS = ("json re collections asyncio datetime typing enum dataclasses functools itertools "
        "decimal statistics zoneinfo tomllib html.parser calendar "
        "json+re+collections+asyncio+datetime").split()


def ir(mods, n):
    cmd = ["valgrind", "--tool=callgrind", "--callgrind-out-file=/dev/null", str(IMP), "ir", mods, str(n)]
    p = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return int(re.search(r"Collected : (\d+)", p.stderr).group(1))


def imp(*args, pin=False):
    cmd = (["taskset", "-c", "2"] if pin else []) + [str(IMP), *map(str, args)]
    return json.loads(subprocess.run(cmd, capture_output=True, text=True, check=True).stdout)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--mods", default=",".join(MODS))
    ap.add_argument("--reps", type=int, default=51)
    ap.add_argument("--no-ir", action="store_true")
    ap.add_argument("--no-wall", action="store_true", help="Ir only (any machine load)")
    a = ap.parse_args()
    mods = ["-"] + [m.replace("+", ",") for m in a.mods.split(",")]
    res = {m: {} for m in mods}
    if not a.no_ir:
        with concurrent.futures.ThreadPoolExecutor(os.cpu_count()) as ex:
            jobs = {(m, n): ex.submit(ir, m, n) for m in mods for n in (1, 11)}
            for m in mods:
                res[m]["steady_ir"] = (jobs[m, 11].result() - jobs[m, 1].result()) / 10
                res[m]["first_ir"] = jobs[m, 1].result()
    for m in mods:
        if a.no_wall:
            res[m].update(median_ns=0, first_ns=0, rss_kib=0)
            continue
        t = imp("time", m, a.reps, pin=True)
        res[m].update(t)
        r = imp("rss", m, 40)
        res[m]["rss_kib"] = (r["after_kib"] - r["before_kib"]) / 40
    base = res["-"]
    print("| import | steady Ir (M) | wall median (µs) | first in process (µs) | RSS/session (KiB) |")
    print("|---|---:|---:|---:|---:|")
    for m in mods:
        r = res[m]
        irs = f"{(r['steady_ir'] - (base['steady_ir'] if m != '-' else 0)) / 1e6:.2f}" if "steady_ir" in r else "-"
        wall = (r["median_ns"] - (base["median_ns"] if m != "-" else 0)) / 1e3
        print(f"| {m} | {irs} | {wall:.0f} | {r['first_ns'] / 1e3:.0f} | {r['rss_kib']:.0f} |")
    Path(a.out).write_text(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()

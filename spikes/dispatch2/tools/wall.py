#!/usr/bin/env python3
"""Pinned, interleaved whole-process wall times (startup included).

usage: tools/wall.py [reps] [out.txt]   env: MM, QJS, D2, CPU (default 2), PROGS
Each round runs every (engine, program) once, in one fixed order, pinned
with `taskset -c $CPU`; one untimed warm-up round first. Every run's
stdout is checked against the expected line. Prints medians and min/max.
"""
import os
import statistics
import subprocess
import sys
import time

here = os.path.dirname(os.path.abspath(__file__))
spike = os.path.dirname(here)
root = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=spike, capture_output=True, text=True).stdout.strip()
MM = os.environ.get("MM", f"{root}/target/release/mm")
QJS = os.environ.get("QJS", "/tmp/dquickts/qjs")
D2 = os.environ.get("D2", f"{spike}/target/release/dispatch2")
CPU = os.environ.get("CPU", "2")
PROGS = os.environ.get("PROGS", "fib closure callfn loopfn loop dictint empty").split()
reps = int(sys.argv[1]) if len(sys.argv) > 1 else 21
out = sys.argv[2] if len(sys.argv) > 2 else None

EXPECT = {
    "fib": "196418", "loop": "4499998500000", "closure": "3000000",
    "callfn": "1000000", "loopfn": "1000000", "dictint": "200000 19999900000", "empty": "",
}


def src(p, ext):
    return f"{root}/bench/{p}.{ext}" if p in ("fib", "closure", "loop") else f"{spike}/programs/{p}.{ext}"


cmds = []
for p in PROGS:
    cmds.append(("mm", p, [MM, "run", src(p, "py")]))
    cmds.append(("qjs", p, [QJS, src(p, "js")]))
    cmds.append(("dispatch2", p, [D2, "run", p]))

samples = {(e, p): [] for e, p, _ in cmds}
for r in range(reps + 1):
    for e, p, c in cmds:
        t = time.perf_counter()
        res = subprocess.run(["taskset", "-c", CPU] + c, capture_output=True, text=True)
        dt = (time.perf_counter() - t) * 1e3
        line = res.stdout.strip().removeprefix(f"{p}: ")
        if p == "empty" and e == "dispatch2":
            line = line.removeprefix("None")
        if res.returncode != 0 or line != EXPECT[p]:
            sys.exit(f"{e} {p}: bad output {res.stdout!r} {res.stderr[-300:]!r}")
        if r > 0:
            samples[(e, p)].append(dt)

lines = [f"# {reps} interleaved rounds, taskset -c {CPU}, ms (median min max); load {os.getloadavg()[0]:.2f}"]
lines.append("engine,program,median,min,max")
for (e, p), s in samples.items():
    lines.append(f"{e},{p},{statistics.median(s):.2f},{min(s):.2f},{max(s):.2f}")
lines.append("# raw")
for (e, p), s in samples.items():
    lines.append(f"{e},{p}," + ",".join(f"{x:.2f}" for x in s))
text = "\n".join(lines) + "\n"
if out:
    open(out, "w").write(text)
print(text.split("# raw")[0])

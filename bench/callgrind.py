#!/usr/bin/env python3
"""Instruction counts (callgrind Ir) for the bench programs, per engine.

    python3 bench/callgrind.py [--engine NAME=CMD ...] [--programs a,b] [--keep DIR]

Each engine is a command prefix that takes a program path, for example
`cpython=python3.14` or `mm=target/release/mm run`. Defaults: CPython 3.14
(`$PYTHON`, else `python3.14`) and `target/release/mm run`. Every program
runs twice per engine under `valgrind --tool=callgrind --trace-children=yes`;
its output must match `expected.txt` both times:

- **gated**: from glibc's `__libc_start_main` onward (the program's `main`
  and libc's own few hundred instructions around it), without glibc's
  `pthread_getattr_np`. That function parses `/proc/self/maps`, whose line
  count follows where the binary's segments land, and Rust's runtime calls
  it from inside `main` to find the main thread's stack. The dynamic loader
  before `__libc_start_main` is left out too. This is the number the ±0.5%
  bench rule applies to (AGENTS.md, docs/spec.md §12).
- **whole process**: every instruction, startup included, as before
  2026-11-08 (reported, not gated).

Runs go in parallel (Ir does not depend on load). Prints a Markdown table of
millions of instructions for each and, with two or more engines, each
engine's ratio to the first. `--keep DIR` keeps the callgrind files for
callgrind_annotate.
"""

import argparse
import concurrent.futures
import os
import re
import shlex
import subprocess
import sys
import tempfile

BENCH = os.path.dirname(os.path.abspath(__file__))
FILES = {"json": "json_rows"}
# Collection toggles of the gated count: on entering `__libc_start_main`,
# off inside `pthread_getattr_np` (callgrind starts with collection off when
# a toggle is given).
GATED = ["--toggle-collect=__libc_start_main*", "--toggle-collect=pthread_getattr_np*"]
ORDER = [
    "startup", "fib", "loop", "dict", "str", "json", "closure",
    "classes", "generators", "exceptions", "fmt", "sort", "interp",
]


def expected():
    out = {}
    with open(os.path.join(BENCH, "expected.txt")) as f:
        for line in f:
            if line.startswith("#") or ":" not in line:
                continue
            name, _, rest = line.partition(": ")
            out[name] = rest.rstrip("\n")
    return out


def run(engine, cmd, program, outdir, gated):
    path = os.path.join(BENCH, FILES.get(program, program) + ".py")
    prefix = os.path.join(outdir, f"{engine}.{program}" + (".gated" if gated else ""))
    argv = [
        "valgrind", "--tool=callgrind", "--trace-children=yes",
        f"--callgrind-out-file={prefix}.%p.out",
    ] + (GATED if gated else []) + shlex.split(cmd) + [path]
    p = subprocess.run(argv, capture_output=True, text=True,
                       env=dict(os.environ, PYTHONHASHSEED="0"))
    total = 0
    for line in p.stderr.splitlines():
        m = re.search(r"Collected : (\d+)", line)
        if m:
            total += int(m.group(1))
    return engine, program, gated, p.stdout.strip(), total, p.returncode


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine", action="append", default=[])
    ap.add_argument("--programs", default=",".join(ORDER))
    ap.add_argument("--keep")
    a = ap.parse_args()
    engines = [e.split("=", 1) for e in a.engine] or [
        ["cpython", os.environ.get("PYTHON", "python3.14")],
        ["mm", os.path.join(os.path.dirname(BENCH), "target/release/mm") + " run"],
    ]
    programs = a.programs.split(",")
    want = expected()
    outdir = a.keep or tempfile.mkdtemp(prefix="callgrind-")
    os.makedirs(outdir, exist_ok=True)
    jobs = [(e, c, p, outdir, g) for p in programs for e, c in engines for g in (True, False)]
    ir, bad = {}, False
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count() or 4) as ex:
        for engine, program, gated, out, total, rc in ex.map(lambda j: run(*j), jobs):
            if rc != 0 or out != want.get(program):
                print(f"{engine} {program}: exit {rc}, printed {out!r}", file=sys.stderr)
                bad = True
            if gated and total == 0:
                print(f"{engine} {program}: nothing collected from __libc_start_main "
                      "(not a glibc binary?)", file=sys.stderr)
                bad = True
            ir[engine, program, gated] = total
    names = [e for e, _ in engines]
    for gated, title in ((True, "Gated: from __libc_start_main, without pthread_getattr_np"),
                         (False, "Whole process")):
        print(f"{title} (millions of Ir):\n")
        head = "| Program | " + " | ".join(names)
        head += "".join(f" | {n} / {names[0]}" for n in names[1:]) + " |"
        print(head)
        print("|---" + "|---:" * (2 * len(names) - 1) + "|")
        for p in programs:
            row = [f"{ir[n, p, gated] / 1e6:,.3f}" for n in names]
            row += [f"{ir[n, p, gated] / max(ir[names[0], p, gated], 1):.4f}" for n in names[1:]]
            print(f"| {p} | " + " | ".join(row) + " |")
        print()
    if not a.keep:
        subprocess.run(["rm", "-rf", outdir])
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())

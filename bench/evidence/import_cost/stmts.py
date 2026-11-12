"""Where a bundled module's body spends its time, by top-level statement.

    python3.14 -I bench/evidence/import_cost/stmts.py MODULE [--reps N] [--top K]

Splits `crates/mm-stdlib/python/<module>.py` (or `<module>/__init__.py`)
into top-level statements, runs them one cell each in a fresh session
(`imp stmts`, pinned to core 2) and prints the K slowest with their median
wall µs. A statement's time includes the modules it imports for the first
time. The body runs as `__main__` code here, so its globals are the cell's;
the bundled module's own decode is not included.
"""
import argparse
import ast
import json
import os
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
IMP = Path(os.environ.get("IMP", ROOT / "target/import-cost/release/imp"))
PY = ROOT / "crates/mm-stdlib/python"


def source(mod):
    p = PY / (mod.replace(".", "/") + ".py")
    return p if p.exists() else PY / mod.replace(".", "/") / "__init__.py"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("module")
    ap.add_argument("--reps", type=int, default=11)
    ap.add_argument("--top", type=int, default=15)
    a = ap.parse_args()
    src = source(a.module).read_text()
    lines = src.splitlines(keepends=True)
    tree = ast.parse(src)
    chunks = []
    for node in tree.body:
        start = (node.decorator_list[0].lineno if getattr(node, "decorator_list", None) else node.lineno) - 1
        chunks.append("".join(lines[start:node.end_lineno]))
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(chunks, f)
    out = subprocess.run(["taskset", "-c", "2", str(IMP), "stmts", f.name, str(a.reps)],
                         capture_output=True, text=True)
    os.unlink(f.name)
    if out.returncode:
        raise SystemExit(out.stderr)
    times = json.loads(out.stdout)
    total = sum(times)
    print(f"{a.module}: {len(chunks)} statements, {total / 1e3:.0f} µs in total")
    for t, c in sorted(zip(times, chunks), reverse=True)[: a.top]:
        first = c.strip().splitlines()[0][:90]
        print(f"{t / 1e3:8.1f} µs  {100 * t / total:4.1f}%  {first}")


if __name__ == "__main__":
    main()

"""Where an import's instructions go, per module: decode, load, prelude, body.

    python3.14 -I bench/evidence/import_cost/split.py [--mods a,b] [--n N]

Runs `imp ir MODS N` (the symbol build: target/import-cost-sym, or `$IMP_SYM`)
under callgrind and splits the Ir of the N imports by callee, inclusive:

- decode: `StdModule::decode` (unpacking and decoding bundled bytecode);
- prelude: the larger of `Vm::prelude_unit` and `prelude_get_all`
  (prelude units built on first use, with their own unpack, decode and
  load; an approximation where both run);
- load + body: the rest of `Vm::execute`: code objects, constants and names
  loaded into the session (`Vm::load_in`, also shown alone, from any
  caller), module bodies, classes, functions and what they call.

Prints millions of Ir per import (the N-run total / N, minus Session::new).
"""
import argparse
import os
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
IMP = Path(os.environ.get("IMP_SYM", ROOT / "target/import-cost-sym/release/imp"))
MODS = ("json re collections asyncio datetime typing enum dataclasses functools itertools "
        "decimal statistics zoneinfo tomllib html.parser calendar "
        "json+re+collections+asyncio+datetime").split()


def incl(lines, fn):
    """Inclusive Ir of `fn`: the largest of the file:function entries
    inlining splits it into (they overlap, so they do not add up)."""
    total = 0
    for line in lines:
        m = re.match(r"\s*([\d,]+) \([\d. %]+\)\s+\S*?:(.*?)\s*$", line)
        if m and m.group(2) == fn:
            total = max(total, int(m.group(1).replace(",", "")))
    return total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mods", default=",".join(MODS))
    ap.add_argument("--n", type=int, default=5)
    a = ap.parse_args()
    print("| import | total | decode | prelude | load + body | of which load_in (any caller) |")
    print("|---|---:|---:|---:|---:|---:|")
    for mod in a.mods.split(","):
        mods = mod.replace("+", ",")
        with tempfile.TemporaryDirectory() as d:
            out = os.path.join(d, "cg.out")
            subprocess.run(["valgrind", "--tool=callgrind", f"--callgrind-out-file={out}",
                            str(IMP), "ir", mods, str(a.n)], capture_output=True, check=True)
            flat = subprocess.run(["callgrind_annotate", "--inclusive=yes", "--threshold=100", out],
                                  capture_output=True, text=True).stdout
        fl = [l.split(" [")[0] for l in flat.splitlines()]
        start = incl(fl, "<mm_engine::vm::Vm>::execute")
        decode = incl(fl, "<mm_engine::vm::StdModule>::decode")
        load = incl(fl, "<mm_engine::vm::Vm>::load_in")
        prelude = max(incl(fl, "<mm_engine::vm::Vm>::prelude_unit"), incl(fl, "<mm_engine::vm::Vm>::prelude_get_all"))
        body = start - decode - prelude
        n = a.n
        print(f"| {mods} | {start / n / 1e6:.2f} | {decode / n / 1e6:.2f} | {prelude / n / 1e6:.2f} | "
              f"{body / n / 1e6:.2f} | {load / n / 1e6:.2f} |")


if __name__ == "__main__":
    main()

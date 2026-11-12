#!/usr/bin/env python3
"""Machine-code size of each variant's dispatch code, from an unstripped build.

Usage: python3 tools/handler_sizes.py target-syms/release/dispatch

- match variant:   size of `dispatch::<design>::run_match` (every handler is
                   `#[inline(always)]` and inlined into it).
- fntable variant: `run_fntable` plus every distinct function the design's
                   `TABLE` points to (read from the table's R_X86_64_RELATIVE
                   relocations, so functions LLVM merged across designs are
                   still counted for every design that uses them).
- shared runtime:  out-of-line `dispatch::rt::*` and `Arena::alloc_slow`
                   (slow paths, dict/list helpers, natives), identical for all.
"""

import re
import subprocess
import sys

TABLE_BYTES = 256 * 8


def main() -> None:
    binary = sys.argv[1]
    nm = subprocess.run(
        ["nm", "--size-sort", "-S", "-C", "--defined-only", binary],
        check=True, capture_output=True, text=True,
    ).stdout
    syms = {}  # addr -> (size, name)
    by_name = {}
    for line in nm.splitlines():
        parts = line.split(" ", 3)
        if len(parts) != 4:
            continue
        addr, size, kind, name = int(parts[0], 16), int(parts[1], 16), parts[2], parts[3]
        if kind.lower() == "t":
            syms.setdefault(addr, (size, name))
        by_name[name] = (addr, size)

    relocs = {}
    rel = subprocess.run(["readelf", "-rW", binary], check=True, capture_output=True, text=True).stdout
    for line in rel.splitlines():
        m = re.match(r"\s*([0-9a-f]+)\s+[0-9a-f]+\s+R_X86_64_RELATIVE\s+([0-9a-f]+)", line)
        if m:
            relocs[int(m.group(1), 16)] = int(m.group(2), 16)

    print("variant,bytes,functions,detail")
    for design in ["stack", "acc", "reg3"]:
        label = {"stack": "stack", "acc": "accumulator", "reg3": "register3"}[design]
        _, msize = by_name[f"dispatch::{design}::run_match"]
        print(f"{label}/match,{msize},1,run_match")
        taddr, _ = by_name[f"dispatch::{design}::TABLE"]
        targets = {relocs[o] for o in range(taddr, taddr + TABLE_BYTES, 8) if o in relocs}
        handlers = sorted((syms[t][1], syms[t][0]) for t in targets)
        _, lsize = by_name[f"dispatch::{design}::run_fntable"]
        total = lsize + sum(s for _, s in handlers)
        foreign = [n for n, _ in handlers if not n.startswith(f"dispatch::{design}::")]
        detail = f"run_fntable {lsize} + {len(handlers)} handler fns"
        if foreign:
            detail += " (incl. merged/shared: " + " ".join(n.split("::", 1)[1] for n in foreign) + ")"
        print(f"{label}/fntable,{total},{len(handlers) + 1},{detail}")

    shared = [(n, s) for a, (s, n) in syms.items()
              if n.startswith("dispatch::rt::") or n == "<dispatch::heap::Arena>::alloc_slow"]
    print(f"shared-runtime,{sum(s for _, s in shared)},{len(shared)},"
          + " ".join(f"{n.split('::')[-1]}={s}" for n, s in sorted(shared)))


if __name__ == "__main__":
    main()

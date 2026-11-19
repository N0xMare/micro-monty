#!/usr/bin/env python3
"""Which functions keep the inline (`_hot`) forms of mm-engine's small `Vm`
helpers, derived from the shipped `mm` and the bench programs
(`crates/mm-engine/tests/hot_accessors.rs`).

    uv run --with pyelftools python3 bench/hot_set.py [--keep DIR]

Builds the shipped native `mm` with symbols and limited debug info
(`target/hot-set/`), counts the bench programs under callgrind
(`bench/callgrind.py`) and `EXTRA`, programs that only this script reads
(`hot_set_ints.py`: the recorded sessions' integer arithmetic past 64
bits, which `replay-ir` judges and no bench program runs), and takes as
hot:

1. the roots: the dispatch loop (`dispatch`, `dispatch_rare`, `execute`)
   and every function whose own instructions are at least `THRESHOLD` of
   some program's, and every function `startup` runs: `native-ir`
   and `wasm-ir` hold startup to a few hundred instructions, which on wasm
   its calls out of line (`builtins.rs::install`) exceed;
2. every function the dispatch loop calls directly (its disassembly): one
   that shrinks can be inlined into `dispatch` (prototype B of the size
   lane: `calliter_next`, fib +4.8%);
3. every function inlined, at any depth, into 1 or 2 (the DWARF inline
   tree).

Then, in `crates/mm-engine/src`, every call of a helper in the test's
`ACCESSORS` from a hot function takes its `_hot` form and every other call
its plain, out-of-line one, and `tests/hot_accessors.txt` is rewritten.
Functions are found by address and named `file.rs::fn` from DWARF (the
declaring file), so two functions of one name in one file (closures, trait
impls) count as one; that can only keep an inline form, never drop one.

A function that shrinks once its calls go out of line can be inlined into
`dispatch` and move its register allocation (fib +8.7% native Ir with every
accessor out of line [M]); run `cargo xtask native-ir`, `wasm-ir` and
`v8-ir` after regenerating.
"""

import argparse
import collections
import os
import pathlib
import re
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
CRATE = ROOT / "crates" / "mm-engine"
# A function is a root when its own instructions are at least this share of
# some program's (callgrind self cost, gated count).
THRESHOLD = 0.0005
# Programs in bench/ that only this script counts.
EXTRA = ["hot_set_ints"]
DISPATCH = {"dispatch", "dispatch_rare", "execute"}


def short(name):
    """`f` of a demangled `<a::B<..>>::f::{closure#0}`."""
    name = re.sub(r"::\{[^}]*\}", "", name)
    while True:
        new = re.sub(r"<[^<>]*>", "", name)
        if new == name:
            break
        name = new
    return name.split("::")[-1].split("<")[0]


def build():
    env = dict(os.environ, RUSTC_BOOTSTRAP="1",
               RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort "
                         "-Cforce-unwind-tables=no --cfg mm_shipped")
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    host = next(line.split()[1] for line in subprocess.run(
        ["rustc", "-vV"], capture_output=True, text=True, check=True).stdout.splitlines()
        if line.startswith("host: "))
    target = ROOT / "target" / "hot-set"
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "-Zbuild-std=std,panic_abort",
         "--target", host, "--target-dir", str(target),
         "--config", "profile.release.strip=false",
         "--config", 'profile.release.debug="limited"',
         "--package", "mm-cli", "--bin", "mm"], cwd=ROOT, env=env, check=True)
    return target / host / "release" / "mm"


def instr_costs(path):
    """{address: Ir} of one callgrind file written with `--dump-instr=yes`,
    and its total."""
    cost = collections.Counter()
    total, addr, skip = 0, 0, False
    for line in open(path, errors="replace"):
        if line.startswith("calls="):
            skip = True
            continue
        c = line[:1]
        if not (c in "+-*" or line.startswith("0x")):
            continue
        p = line.split()
        t = p[0]
        addr = addr + int(t, 0) if t[0] in "+-" else addr if t == "*" else int(t, 16)
        if skip:
            skip = False
            continue
        if len(p) >= 3:
            ir = int(p[2])
            cost[addr] += ir
            total += ir
    return cost, total


def dwarf(mm):
    """The functions of `mm` by address, `{low_pc: (key, die)}`, a key being
    `file.rs::fn` (the declaring file's name and the function's), and a
    function that gives an inlined call site its key."""
    from elftools.elf.elffile import ELFFile

    fh = open(mm, "rb")  # pyelftools reads lazily; kept open for the run
    di = ELFFile(fh).get_dwarf_info()
    programs = {}

    def files(cu):
        if cu.cu_offset not in programs:
            lp = di.line_program_for_CU(cu)
            programs[cu.cu_offset] = (lp["file_entry"], lp["version"]) if lp else ([], 5)
        return programs[cu.cu_offset]

    def key(die):
        name = decl = None
        for _ in range(6):
            a = die.attributes
            if name is None and "DW_AT_name" in a:
                v = a["DW_AT_name"].value
                name = v.decode() if isinstance(v, bytes) else v
            if decl is None and "DW_AT_decl_file" in a:
                entries, version = files(die.cu)
                i = a["DW_AT_decl_file"].value - (0 if version >= 5 else 1)
                if 0 <= i < len(entries):
                    n = entries[i].name
                    decl = (n.decode() if isinstance(n, bytes) else n).rsplit("/", 1)[-1]
            if name is not None and decl is not None:
                break
            for o in ("DW_AT_abstract_origin", "DW_AT_specification"):
                if o in a:
                    die = die.get_DIE_from_attribute(o)
                    break
            else:
                break
        # A closure counts as the function it is written in (its namespace).
        parent = die
        while name and name.startswith("{") and parent is not None:
            parent = parent.get_parent()
            if parent is not None and "DW_AT_name" in parent.attributes:
                v = parent.attributes["DW_AT_name"].value
                name = v.decode() if isinstance(v, bytes) else v
        return f"{decl or '?'}::{short(name or '?')}"

    rl = di.range_lists()

    def rng(die):
        a = die.attributes
        if "DW_AT_low_pc" in a and "DW_AT_high_pc" in a:
            lo, hi = a["DW_AT_low_pc"].value, a["DW_AT_high_pc"]
            return [(lo, lo + hi.value if hi.form != "DW_FORM_addr" else hi.value)]
        out = []
        if "DW_AT_ranges" in a and rl is not None:
            top = die.cu.get_top_DIE().attributes.get("DW_AT_low_pc")
            base = top.value if top else 0
            for r in rl.get_range_list_at_offset(a["DW_AT_ranges"].value, cu=die.cu):
                if hasattr(r, "begin_address"):
                    out.append((r.begin_address, r.end_address))
                elif hasattr(r, "begin_offset") and r.begin_offset < r.end_offset:
                    out.append((base + r.begin_offset, base + r.end_offset))
        return out

    global ranges
    ranges = rng
    funcs = {}
    for cu in di.iter_CUs():
        stack = [cu.get_top_DIE()]
        while stack:
            for c in stack.pop().iter_children():
                if c.tag == "DW_TAG_subprogram" and "DW_AT_low_pc" in c.attributes:
                    funcs[c.attributes["DW_AT_low_pc"].value] = (key(c), c)
                elif c.tag in ("DW_TAG_namespace", "DW_TAG_structure_type",
                               "DW_TAG_enumeration_type", "DW_TAG_union_type"):
                    stack.append(c)
    return funcs, key


def measured(mm, keep, funcs, key, helpers):
    """Every `file.rs::fn` of mm-engine that runs at least `THRESHOLD` of
    some program's instructions, and every one `startup` runs.
    An instruction counts for the innermost function of its DWARF inline
    chain that mm-engine declares and that is not one of the helpers, so
    std's, mm-core's and the helpers' code inlined into a function counts
    for it."""
    outdir = pathlib.Path(keep or tempfile.mkdtemp(prefix="hot-set-"))
    outdir.mkdir(parents=True, exist_ok=True)
    sys.path.insert(0, str(ROOT / "bench"))
    import callgrind

    progs = callgrind.ORDER + EXTRA

    def count(prog):
        src = ROOT / "bench" / (callgrind.FILES.get(prog, prog) + ".py")
        subprocess.run(["valgrind", "--tool=callgrind", "--dump-instr=yes",
                        f"--callgrind-out-file={outdir}/{prog}.out",
                        *callgrind.GATED, str(mm), "run", str(src)],
                       check=True, capture_output=True,
                       env=dict(os.environ, PYTHONHASHSEED="0"))
        return instr_costs(outdir / f"{prog}.out")

    import concurrent.futures
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count() or 4) as ex:
        runs = list(ex.map(count, progs))
    # (lo, hi, depth, key) of every concrete function and inlined call.
    spans = []
    for lo, (k, die) in funcs.items():
        hi = die.attributes.get("DW_AT_high_pc")
        if hi is None:
            continue
        spans.append((lo, lo + hi.value if hi.form != "DW_FORM_addr" else hi.value, 0, k))
        todo = [(die, 0)]
        while todo:
            d, depth = todo.pop()
            for c in d.iter_children():
                if c.tag == "DW_TAG_inlined_subroutine":
                    for a, b in ranges(c):
                        spans.append((a, b, depth + 1, key(c)))
                    todo.append((c, depth + 1))
                elif c.tag == "DW_TAG_lexical_block":
                    todo.append((c, depth))
    spans.sort()

    def owner(k):
        f, _, n = k.partition("::")
        return f in ENGINE_FILES and n not in helpers

    out = set()
    import heapq
    for prog, (cost, total) in zip(progs, runs):
        per = collections.Counter()
        active, j = [], 0
        for addr in sorted(cost):
            while j < len(spans) and spans[j][0] <= addr:
                lo, hi, depth, k = spans[j]
                heapq.heappush(active, (hi, -depth, j, k))
                j += 1
            while active and active[0][0] <= addr:
                heapq.heappop(active)
            chain = sorted((d, k) for hi, d, _, k in active if hi > addr)
            who = next((k for _, k in chain if owner(k)), None)
            if who:
                per[who] += cost[addr]
        floor = 1 if prog == "startup" else THRESHOLD * total
        out |= {k for k, ir in per.items() if ir >= floor}
    return out


def direct_callees(mm, at):
    """Addresses that the functions at addresses `at` call."""
    dis = subprocess.run(["objdump", "-d", "--no-show-raw-insn", str(mm)],
                         capture_output=True, text=True, check=True).stdout
    out, cur = set(), None
    for line in dis.splitlines():
        m = re.match(r"^([0-9a-f]+) <.*>:$", line)
        if m:
            cur = int(m.group(1), 16)
            continue
        if cur in at:
            m = re.search(r"call\s+([0-9a-f]+) <", line)
            if m:
                out.add(int(m.group(1), 16))
    return out


def inlined_into(funcs, key, at):
    """Keys of the functions at `at` and of all inlined into them."""
    out = set()
    for a in at:
        if a not in funcs:
            continue
        k, die = funcs[a]
        out.add(k)
        todo = [die]
        while todo:
            for c in todo.pop().iter_children():
                if c.tag == "DW_TAG_inlined_subroutine":
                    out.add(key(c))
                    todo.append(c)
                elif c.tag == "DW_TAG_lexical_block":
                    todo.append(c)
    return out


def accessors():
    test = (CRATE / "tests" / "hot_accessors.rs").read_text()
    block = test[test.index("const ACCESSORS"):]
    return re.findall(r'"(\w+)"', block[: block.index("];")])


ENGINE_FILES = {p.name for p in (CRATE / "src").glob("*.rs")}
FNDEF = re.compile(r"^\s*(?:pub(?:\([\w:]+\))? )?(?:const )?fn (\w+)")


def apply(hot, names):
    """Rewrites the helpers' calls by `hot` (`None`: every call takes the
    `_hot` form) and returns the allowlist."""
    alt = "|".join(names)
    # Receivers are `self` or `vm` (a `Vm`), also when rustfmt puts the
    # call on the line after its receiver; `self.heap.intern(` is not one.
    call = re.compile(r"\b(self|vm)\.(" + alt + r")(?:_hot)?\(")
    cont = re.compile(r"^(\s*)\.(" + alt + r")(?:_hot)?\(")
    defs = set(names) | {n + "_hot" for n in names}
    allow = set()
    for path in sorted((CRATE / "src").glob("*.rs")):
        if path.name.endswith("_tests.rs"):
            continue
        lines = path.read_text().split("\n")
        cur, changed = None, False
        for i, line in enumerate(lines):
            if line.lstrip().startswith("#[cfg(test)]"):
                break
            m = FNDEF.match(line)
            if m:
                cur = m.group(1)
            if cur in defs:
                # The definitions: a plain one calls its `_hot` body, and a
                # `_hot` body is inlined into hot callers, so it is hot.
                continue
            want = "_hot" if hot is None or f"{path.name}::{cur}" in hot else ""
            new = call.sub(lambda mm: f"{mm.group(1)}.{mm.group(2)}{want}(", line)
            if i and re.search(r"\b(self|vm)$", lines[i - 1].rstrip()):
                new = cont.sub(lambda mm: f"{mm.group(1)}.{mm.group(2)}{want}(", new)
            if new != line:
                lines[i], changed = new, True
            if want and re.search(r"\.(" + alt + r")_hot\(", new):
                allow.add(f"{path.name}::{cur}")
        if changed:
            path.write_text("\n".join(lines))
    return allow


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--keep", help="keep the callgrind files in this directory")
    a = ap.parse_args()
    # Measured on the tree with every call inline, as the code was before
    # the split: a function whose calls went out of line is smaller and
    # may be inlined where it was not, so measuring a split tree would
    # depend on the last split.
    names = accessors()
    apply(None, names)
    mm = build()
    funcs, key = dwarf(mm)
    dispatch = {x for x, (k, _) in funcs.items() if k == "interp.rs::dispatch"}
    helpers = set(names) | {n + "_hot" for n in names}
    hot = measured(mm, a.keep, funcs, key, helpers)
    hot |= inlined_into(funcs, key, dispatch | direct_callees(mm, dispatch))
    allow = apply(hot, names)
    (CRATE / "tests" / "hot_accessors.txt").write_text("".join(f"{x}\n" for x in sorted(allow)))
    print(f"{len(hot)} hot functions, {len(allow)} keep `_hot` calls")
    subprocess.run(["cargo", "fmt", "--all"], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()

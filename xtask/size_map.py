#!/usr/bin/env python3
"""Size map of the shipped builds (docs/research/size-map.md).

Builds `hello` (wasm32-unknown-unknown, wasm32-wasip1, native) and the
mm-wasm guest exactly as `cargo xtask size` does (xtask/src/main.rs
`shipped_cargo`), except that symbol names and line tables are kept
(`strip = false`, `debug = "line-tables-only"`) and the wasm linker writes a
map. The script then checks that the code is the shipped code (wasm: the
code section is byte-identical to the shipped pre-`wasm-opt` module, and
`wasm-opt -Oz -g` on the DWARF-stripped module gives the shipped size plus
the name section; native: `.text`/`.rodata` sizes equal the shipped binary's)
and attributes every byte:

- code per function (name section / ELF symbols), per crate and per
  mm-engine module, and per source file through the DWARF line tables, so
  code inlined into `Vm::execute` is counted where it was written;
- data per owner: each bundled stdlib module's packed bytecode, the shared
  Huffman header, the prelude chunks (all found by content from the build
  scripts' OUT_DIR blobs), named statics from the link map / symbol table,
  and the rest classified as string constants or other constants;
- twiggy `top`, `dominators`, `paths`, `monos` and `garbage` on the named
  wasm modules (text files in the output directory).

Usage (from the repository root):

    uv run --with pyelftools python3 xtask/size_map.py [--skip-build]
        [--out target/size-map/report] [--removals | --only-removals]
        [--markdown]

`--removals` adds removal builds of `hello` (wasm and native: one stdlib
feature off at a time, all of them, unicodedata on, the facade without
`test-hooks`) in a git worktree under target/size-map, built and measured
as shipped (stripped). `--markdown` only re-renders size-map.md from the
JSON files. Needs `twiggy` (`cargo install twiggy`; optional), `wasm-opt`
(as `cargo xtask size`), binutils `nm` and `c++filt` (Rust v0 demangling),
and pyelftools. Results: docs/research/size-map.md.
"""

import argparse
import bisect
import collections
import io
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET_DIR = ROOT / "target" / "size-map"
UU = "wasm32-unknown-unknown"
WASI = "wasm32-wasip1"
NATIVE = "x86_64-unknown-linux-gnu"
WASM_FEATURES = [
    "--enable-bulk-memory",
    "--enable-bulk-memory-opt",
    "--enable-call-indirect-overlong",
    "--enable-multivalue",
    "--enable-mutable-globals",
    "--enable-nontrapping-float-to-int",
    "--enable-reference-types",
    "--enable-sign-ext",
]


def tool(var, name):
    if os.environ.get(var):
        return os.environ[var]
    local = Path.home() / ".local" / "bin" / name
    if local.exists():
        return str(local)
    found = shutil.which(name)
    if not found:
        sys.exit(f"{name} not found (${var}, ~/.local/bin, PATH)")
    return found


def run(cmd, **kw):
    print("+", " ".join(str(c) for c in cmd), file=sys.stderr)
    subprocess.run(cmd, check=True, **kw)


# --------------------------------------------------------------------------
# Builds


def shipped_cargo(subcommand, target, root=ROOT, target_dir=TARGET_DIR, named=True):
    """`shipped_cargo` of xtask/src/main.rs, plus names and line tables."""
    env = dict(os.environ)
    env["RUSTC_BOOTSTRAP"] = "1"
    env["RUSTFLAGS"] = "-Zunstable-options -Cpanic=immediate-abort -Cforce-unwind-tables=no --cfg mm_shipped"
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    cmd = [os.environ.get("CARGO", "cargo"), subcommand, "--release", "--locked",
           "-Zbuild-std=std,panic_abort", "--target", target,
           "--target-dir", str(target_dir)]
    if named:
        cmd += ["--config", "profile.release.strip=false",
                "--config", "profile.release.package.micro-monty.strip=false",
                "--config", 'profile.release.debug="line-tables-only"']
    return cmd, env


def link_map_args(target, out):
    if target.startswith("wasm32"):
        return [f"-Clink-arg=--Map={out}"]
    return [f"-Clink-arg=-Wl,-Map={out}"]


def build_hello(target, root=ROOT, target_dir=TARGET_DIR, named=True):
    cmd, env = shipped_cargo("rustc", target, root, target_dir, named)
    cmd += ["--package", "micro-monty", "--example", "hello", "--", "-Copt-level=3"]
    out = target_dir / target / "release" / "examples"
    if named:
        cmd += link_map_args(target, target_dir / f"hello-{target}.map")
    run(cmd, env=env, cwd=root)
    return out / ("hello.wasm" if target.startswith("wasm32") else "hello")


def build_guest(target_dir=TARGET_DIR):
    cmd, env = shipped_cargo("rustc", UU, ROOT, target_dir)
    cmd += ["--package", "mm-wasm-guest", "--lib", "--"]
    cmd += link_map_args(UU, target_dir / "guest.map")
    run(cmd, env=env, cwd=ROOT)
    return target_dir / UU / "release" / "mm_wasm_guest.wasm"


def wasm_opt(src, dst, names):
    cmd = [tool("WASM_OPT", "wasm-opt"), "-Oz", *WASM_FEATURES, str(src), "-o", str(dst)]
    if names:
        cmd.insert(2, "-g")
    run(cmd)


# --------------------------------------------------------------------------
# Wasm parsing


def leb(buf, pos):
    result = shift = 0
    while True:
        b = buf[pos]
        pos += 1
        result |= (b & 0x7F) << shift
        shift += 7
        if b < 0x80:
            return result, pos


def wasm_sections(buf):
    """[(id, name, payload_start, payload_end, section_start)]"""
    assert buf[:4] == b"\0asm"
    pos = 8
    out = []
    while pos < len(buf):
        start = pos
        sid = buf[pos]
        size, pos = leb(buf, pos + 1)
        end = pos + size
        name = None
        if sid == 0:
            n, p = leb(buf, pos)
            name = buf[p:p + n].decode()
        out.append((sid, name, pos, end, start))
        pos = end
    return out


SECTION_NAMES = {1: "type", 2: "import", 3: "function", 4: "table", 5: "memory", 6: "global",
                 7: "export", 8: "start", 9: "elem", 10: "code", 11: "data", 12: "datacount"}


def strip_custom(buf, keep=lambda name: False):
    out = bytearray(buf[:8])
    for sid, name, p, e, s in wasm_sections(buf):
        if sid == 0 and not keep(name):
            continue
        out += buf[s:e]
    return bytes(out)


def wasm_functions(buf):
    """Function bodies: [(index, body_start, body_end)] (file offsets, size
    including the body's size prefix), names {index: name}, number of
    imported functions, and data segments [(offset_in_memory, file_start, file_end)]."""
    secs = wasm_sections(buf)
    imported = 0
    bodies, names, segments = [], {}, []
    for sid, name, p, e, s in secs:
        if sid == 2:
            count, q = leb(buf, p)
            for _ in range(count):
                for _ in range(2):
                    n, q = leb(buf, q)
                    q += n
                kind = buf[q]
                q += 1
                if kind == 0:
                    _, q = leb(buf, q)
                    imported += 1
                elif kind == 1:
                    q += 1
                    flags, q = leb(buf, q)
                    _, q = leb(buf, q)
                    if flags & 1:
                        _, q = leb(buf, q)
                elif kind == 2:
                    flags, q = leb(buf, q)
                    _, q = leb(buf, q)
                    if flags & 1:
                        _, q = leb(buf, q)
                elif kind == 3:
                    q += 2
                else:
                    raise ValueError("import kind")
        elif sid == 10:
            count, q = leb(buf, p)
            for i in range(count):
                start = q
                size, q = leb(buf, q)
                q += size
                bodies.append((imported + i, start, q))
        elif sid == 11:
            count, q = leb(buf, p)
            for _ in range(count):
                flags, q = leb(buf, q)
                offset = None
                if flags == 0 or flags == 2:
                    if flags == 2:
                        _, q = leb(buf, q)
                    op = buf[q]
                    q += 1
                    if op == 0x41:  # i32.const
                        v, q = leb(buf, q)
                        offset = v
                    else:
                        raise ValueError("data offset")
                    assert buf[q] == 0x0B
                    q += 1
                size, q = leb(buf, q)
                segments.append((offset, q, q + size))
                q += size
        elif sid == 0 and name == "name":
            n, q = leb(buf, p)
            q += n
            while q < e:
                sub = buf[q]
                size, q = leb(buf, q + 1)
                end = q + size
                if sub == 1:
                    count, r = leb(buf, q)
                    for _ in range(count):
                        idx, r = leb(buf, r)
                        n, r = leb(buf, r)
                        names[idx] = buf[r:r + n].decode("utf-8", "replace")
                        r += n
                q = end
    return secs, bodies, names, imported, segments


# --------------------------------------------------------------------------
# Names


ESCAPES = {"$LT$": "<", "$GT$": ">", "$u20$": " ", "$RF$": "&", "$BP$": "*", "$C$": ",",
           "$u7b$": "{", "$u7d$": "}", "$u5b$": "[", "$u5d$": "]", "$u27$": "'",
           "$u3b$": ";", "$u7e$": "~", "$u21$": "!", "$u22$": '"', "$u23$": "#",
           "$u2b$": "+", "$u3d$": "=", "$u3a$": ":", "$SP$": "@"}
HASH = re.compile(r"::h[0-9a-f]{16}$")


def demangle_legacy(sym):
    """_ZN...E Rust legacy symbols (ELF), and lld's half-demangled names."""
    if sym.startswith("_ZN") and sym.endswith("E"):
        parts, pos, s = [], 3, sym
        while pos < len(s) - 1 and s[pos].isdigit():
            j = pos
            while s[j].isdigit():
                j += 1
            n = int(s[pos:j])
            parts.append(s[j:j + n])
            pos = j + n
        sym = "::".join(parts)
    sym = HASH.sub("", sym)
    for k, v in ESCAPES.items():
        sym = sym.replace(k, v)
    sym = sym.replace("..", "::")
    if sym.startswith("_<"):
        sym = sym[1:]
    sym = sym.replace("::_<", "::<").replace("::_$", "::$")
    return sym


CRATES = {
    "mm_core": "mm-core", "mm_compile": "mm-compile", "mm_parse": "mm-parse",
    "mm_engine": "mm-engine", "mm_stdlib": "mm-stdlib", "micro_monty": "facade",
    "hello": "facade", "mm_wasm_guest": "mm-wasm-guest", "mm_wasm_abi": "mm-wasm-abi",
    "mm_wasm_alloc": "alloc (dlmalloc, mm-wasm-alloc)", "dlmalloc": "alloc (dlmalloc, mm-wasm-alloc)",
    "core": "std/core/alloc", "alloc": "std/core/alloc", "std": "std/core/alloc",
    "compiler_builtins": "compiler_builtins", "hashbrown": "std/core/alloc",
}
OURS = ("mm_core", "mm_compile", "mm_parse", "mm_engine", "mm_stdlib", "micro_monty",
        "mm_wasm_guest", "mm_wasm_abi", "hello")


def demangle_all(names):
    """{mangled: readable} through c++filt (binutils ≥ 2.36 reads Rust v0),
    without the `[crate hash]` suffixes."""
    names = sorted(set(names))
    out = subprocess.run(["c++filt"], input="\n".join(names), capture_output=True,
                         text=True, check=True).stdout.splitlines()
    return {m: re.sub(r"\[[0-9a-f]{1,16}\]", "", d) for m, d in zip(names, out)}


B62 = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"


def v0_owner(sym):
    """(crate, [module idents]) where a Rust v0 symbol's code is written:
    the path of the enclosing impl block for methods (`M`/`X`), so
    `<Vm>::execute` written in `mm_engine::interp` belongs to `interp`."""
    if not sym.startswith("_R"):
        return None
    s = sym[2:].split(".")[0]

    def base62(i):
        if s[i] == "_":
            return 0, i + 1
        j = s.index("_", i)
        n = 0
        for c in s[i:j]:
            n = n * 62 + B62.index(c)
        return n + 1, j + 1

    def ident(i):
        if s[i] == "u":
            i += 1
        j = i
        while s[j].isdigit():
            j += 1
        n = int(s[i:j])
        if s[j] == "_":
            j += 1
        return s[j:j + n], j + n

    def disamb(i):
        if s[i] == "s":
            _, i = base62(i + 1)
        return i

    def path(i, depth=0):
        """-> (crate, chain, final)"""
        if depth > 50:
            raise ValueError
        c = s[i]
        if c == "C":
            i = disamb(i + 1)
            name, i = ident(i)
            return (name, [], False)
        if c == "N":
            ns = s[i + 1]
            crate, chain, final = path(i + 2, depth + 1)
            if final:
                return crate, chain, True
            # the ident follows the inner path; we need its position
            j = skip_path(i + 2)
            j = disamb(j)
            name, _ = ident(j)
            if ns == "t":
                return crate, chain + [name], False
            if ns == "v":
                return crate, chain, True
            return crate, chain, True  # closures, shims: stay with the parent
        if c in "MX":
            j = disamb(i + 1)
            crate, chain, _ = path(j, depth + 1)
            return crate, chain, True
        if c == "Y":
            raise ValueError("trait definition")
        if c == "I":
            return path(i + 1, depth + 1)
        if c == "B":
            n, _ = base62(i + 1)
            return path(n, depth + 1)
        raise ValueError(c)

    def skip_path(i):
        c = s[i]
        if c == "C":
            i = disamb(i + 1)
            return ident(i)[1]
        if c == "N":
            i = skip_path(i + 2)
            i = disamb(i)
            return ident(i)[1]
        if c == "B":
            return base62(i + 1)[1]
        raise ValueError("skip " + c)

    try:
        crate, chain, _ = path(0)
        return crate, chain
    except (ValueError, IndexError):
        return None


def split_path(name):
    """Top-level `::` segments, keeping <...> together."""
    out, depth, cur = [], 0, ""
    i = 0
    while i < len(name):
        c = name[i]
        if c == "<":
            depth += 1
        elif c == ">" and depth:
            depth -= 1
        if depth == 0 and name.startswith("::", i):
            out.append(cur)
            cur = ""
            i += 2
            continue
        cur += c
        i += 1
    out.append(cur)
    return out


def owner(name):
    """(crate key, module path) of the code a function name belongs to.

    `<impl X>`/`<X as T>` methods belong to the crate of the path that
    holds the impl, generic std code instantiated for our types belongs
    to std (its `monos` row says for whom)."""
    segs = split_path(name)
    first = segs[0]
    if first.startswith("<"):
        inner = first[1:-1] if first.endswith(">") else first[1:]
        # `<X as T>`: the impl lives with X if X is ours, else with T.
        m = re.match(r"^(&(mut )?)?(dyn )?([A-Za-z_][\w]*)(::[\w:]+)?", inner)
        tm = re.search(r" as ([A-Za-z_][\w]*)((::[\w]+)*)", inner)
        if m and m.group(4) in OURS:
            segs = split_path(inner.split(" as ")[0].lstrip("&").removeprefix("mut ").removeprefix("dyn "))
        elif tm and tm.group(1) in OURS:
            segs = [tm.group(1)] + tm.group(2).strip(":").split("::")
        elif m:
            segs = split_path(inner.split(" as ")[0].lstrip("&").removeprefix("mut ").removeprefix("dyn "))
        else:
            return "other", ""
        first = segs[0]
    crate = first.split("<")[0]
    mods = []
    for s in segs[1:]:
        if s.startswith("<impl") or s[:1].isupper() or s.startswith("{{") or s.startswith("_{{"):
            break
        mods.append(s.split("<")[0])
    if crate in OURS:
        # Drop the function name itself when the path has no type.
        if mods and segs[len(mods)] == segs[-1]:
            mods = mods[:-1]
    return crate, "::".join(mods)


def classify(raws):
    """{raw name: (readable name, crate group, module path)}."""
    readable = demangle_all(raws)
    out = {}
    for raw in set(raws):
        name = demangle_legacy(readable.get(raw, raw))
        o = v0_owner(raw)
        if o:
            crate, mods = o[0], "::".join(o[1])
        else:
            crate, mods = owner(name)
        g = crate_group(crate)
        if "::" not in name and not o:
            if raw.startswith("mm_"):
                g = "mm-wasm-guest"
            elif raw.startswith("__") or raw in ("memcmp", "memcpy", "memset", "memmove", "fmod", "fmodf", "bcmp"):
                g = "compiler_builtins"
            elif raw in ("main", "_start", "__main_void", "_init", "_fini"):
                g = "std/core/alloc"
        if re.fullmatch(r"func\[\d+\]", raw):
            g = "unnamed (wasm-opt outlined)"
        out[raw] = (name, g, mods)
    return out


def crate_group(crate):
    return CRATES.get(crate, crate if crate else "other")


def mono_key(name):
    """Name with generic argument lists removed (`f::<T>` and `Vec<T>` ->
    `f`, `Vec`); a leading `<X as T>` keeps its shape."""
    out, i = [], 0
    while i < len(name):
        c = name[i]
        prev = out[-1] if out else ""
        if c == "<" and prev and (prev.isalnum() or prev in "_:"):
            depth, j = 0, i
            while j < len(name):
                if name[j] == "<":
                    depth += 1
                elif name[j] == ">":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            if len(out) > 1 and out[-1] == ":" and out[-2] == ":":
                out.pop()
                out.pop()
            i = j + 1
            continue
        out.append(c)
        i += 1
    return "".join(out)


# --------------------------------------------------------------------------
# DWARF line tables (pyelftools), per-file bytes per address range


def ours(path):
    return bool(path) and bool(re.search(r"(^|/)crates/|/out/", path))


def line_rows(dwarfinfo, base=0):
    """[(address, file)] sorted; a sequence end has file None.

    The file is the innermost frame of the inline stack at that address
    that is written in this repository (DW_TAG_inlined_subroutine call
    sites), so a `Vec::push` or `?` inlined into `class.rs` counts for
    `class.rs`; std's file only when no frame of ours encloses it (an
    out-of-line std function)."""
    rows = []
    intervals = []  # (lo, hi, depth, call_file)
    for cu in dwarfinfo.iter_CUs():
        lp = dwarfinfo.line_program_for_CU(cu)
        if lp is None:
            continue
        files = lp["file_entry"]
        dirs = lp["include_directory"]
        version = lp["version"]

        def fname(idx):
            i = idx if version >= 5 else idx - 1
            if i < 0 or i >= len(files):
                return "?"
            fe = files[i]
            d = fe.dir_index if version >= 5 else fe.dir_index - 1
            n = fe.name.decode() if isinstance(fe.name, bytes) else fe.name
            if 0 <= d < len(dirs):
                dn = dirs[d].decode() if isinstance(dirs[d], bytes) else dirs[d]
                if not n.startswith("/"):
                    n = dn.rstrip("/") + "/" + n
            return n

        cache = {}

        def cached(idx):
            f = cache.get(idx)
            if f is None:
                f = cache[idx] = fname(idx)
            return f

        for entry in lp.get_entries():
            st = entry.state
            if st is None:
                continue
            rows.append((st.address + base, None if st.end_sequence else (cached(st.file), st.line)))
        # inline call sites
        rl = dwarfinfo.range_lists()
        top = cu.get_top_DIE()
        cu_low = top.attributes.get("DW_AT_low_pc")
        cu_base = cu_low.value if cu_low else 0
        stack = [(top, 0)]
        while stack:
            die, depth = stack.pop()
            for child in die.iter_children():
                tag = child.tag
                if tag == "DW_TAG_inlined_subroutine":
                    a = child.attributes
                    cf = a.get("DW_AT_call_file")
                    ranges = []
                    if "DW_AT_low_pc" in a:
                        lo = a["DW_AT_low_pc"].value
                        hi = a["DW_AT_high_pc"]
                        hi = lo + hi.value if hi.form != "DW_FORM_addr" else hi.value
                        ranges.append((lo, hi))
                    elif "DW_AT_ranges" in a and rl is not None:
                        try:
                            for r in rl.get_range_list_at_offset(a["DW_AT_ranges"].value, cu=child.cu):
                                if hasattr(r, "begin_offset"):
                                    if r.begin_offset < r.end_offset:
                                        ranges.append((cu_base + r.begin_offset, cu_base + r.end_offset))
                                elif hasattr(r, "begin_address"):
                                    ranges.append((r.begin_address, r.end_address))
                        except Exception:
                            pass
                    f = cached(cf.value) if cf else "?"
                    for lo, hi in ranges:
                        cl = a.get("DW_AT_call_line")
                        intervals.append((lo + base, hi + base, depth + 1, (f, cl.value if cl else 0)))
                    stack.append((child, depth + 1))
                elif tag in ("DW_TAG_subprogram", "DW_TAG_lexical_block", "DW_TAG_namespace",
                             "DW_TAG_structure_type", "DW_TAG_enumeration_type", "DW_TAG_union_type"):
                    stack.append((child, depth))
    rows.sort(key=lambda r: (r[0], r[1] is not None))
    # Re-attribute rows in std files to the innermost enclosing call site
    # in our files: sweep rows and intervals in address order.
    intervals.sort()
    import heapq
    active = []  # heap of (hi, -depth, call_file, lo)
    j = 0
    out = []
    for addr, f in rows:
        while j < len(intervals) and intervals[j][0] <= addr:
            lo, hi, depth, cf = intervals[j]
            heapq.heappush(active, (hi, -depth, j, cf, lo))
            j += 1
        while active and active[0][0] <= addr:
            heapq.heappop(active)
        if f is None:
            out.append((addr, None))
            continue
        # The frames, innermost first: the row's file, then the call site of
        # the deepest inlined frame (in its caller), and so on outwards.
        chain = [f] + [x[3] for x in sorted(active, key=lambda x: (x[1], x[2]))]
        inner = next((c for c in chain if ours(c[0])), f)
        user = next((c for c in chain if ours(c[0]) and "mm-core/" not in c[0]), inner)
        frames = tuple(dict.fromkeys(fn_key(c[0], c[1]) for c in chain if ours(c[0])))
        out.append((addr, (inner[0], user[0], user[1], frames)))
    return out


def file_bytes(rows, addrs, lo, hi):
    """{file: bytes} for [lo, hi) from sorted line rows (`addrs` their
    addresses)."""
    out = collections.Counter()
    if not rows:
        out["?"] = hi - lo
        return out
    i = bisect.bisect_right(addrs, lo) - 1
    pos = lo
    cur = rows[i][1] if i >= 0 else None
    i += 1
    while pos < hi:
        nxt = rows[i][0] if i < len(rows) else hi
        end = min(nxt, hi)
        if end > pos:
            out[cur or "?"] += end - pos
            pos = end
        if i < len(rows):
            cur = rows[i][1]
        i += 1
        if i > len(rows) and pos < hi:
            out[cur or "?"] += hi - pos
            break
    return out


_FNS = {}
_ARMS = {}
_KEYS = {}


def fn_key(path, line):
    """`crates/<crate>/src/<file>::<fn>` for a repository source line; in
    the interpreter's dispatch functions, `::dispatch[op::NAME]` per arm."""
    k = (path, line)
    if k not in _KEYS:
        m = re.search(r"(?:^|/)(crates/.*)$", path or "")
        rel = m.group(1) if m else path
        fn = rust_fn_at(path, line)
        if fn in ("dispatch", "dispatch_rare") and rel not in _ARMS:
            arms = []
            for i, text in enumerate((ROOT / rel).read_text().splitlines(), 1):
                am = re.match(r"^\s*(op::[A-Z0-9_]+(?:\s*\|\s*op::[A-Z0-9_]+)*)\s*=>", text)
                if am:
                    arms.append((i, re.sub(r"\s+", "", am.group(1))))
            _ARMS[rel] = arms
        if fn in ("dispatch", "dispatch_rare"):
            arms = _ARMS[rel]
            a = bisect.bisect_right(arms, (line, "\uffff")) - 1
            if a >= 0:
                fn = f"{fn}[{arms[a][1]}]"
        _KEYS[k] = f"{rel}::{fn}"
    return _KEYS[k]


def rust_fn_at(path, line):
    """The `fn` item a repository source line is in (the last `fn` that
    starts at or before it), from the source text."""
    m = re.search(r"(?:^|/)(crates/.*)$", path or "")
    if not m:
        return None
    rel = m.group(1)
    if rel not in _FNS:
        starts = []
        try:
            for i, text in enumerate((ROOT / rel).read_text().splitlines(), 1):
                fm = re.match(r"^\s*(?:pub(?:\([\w:]+\))? )?(?:const )?(?:async )?(?:unsafe )?fn (\w+)", text)
                if fm:
                    starts.append((i, fm.group(1)))
        except OSError:
            pass
        _FNS[rel] = starts
    starts = _FNS[rel]
    k = bisect.bisect_right(starts, (line, "\uffff")) - 1
    return starts[k][1] if k >= 0 else "<top>"


def source_group(path):
    """Crate and file for a source path from the line tables."""
    if path in ("?", None):
        return "unknown", "?"
    m = re.search(r"(?:^|/)crates/([^/]+)/src/(.*)$", path)
    if m:
        crate = {"micro-monty": "facade"}.get(m.group(1), m.group(1))
        if m.group(1) == "mm-wasm-alloc":
            crate = "alloc (dlmalloc, mm-wasm-alloc)"
        return crate, m.group(2)
    m = re.search(r"(?:^|/)crates/([^/]+)/examples/(.*)$", path)
    if m:
        return "facade", "examples/" + m.group(2)
    if path.startswith("src/") or path.startswith("library/"):
        return "std/core/alloc", path
    if "/out/" in path:
        m = re.search(r"/build/([a-z0-9_-]+?)-[0-9a-f]{16}/out/(.*)$", path)
        if m:
            return {"micro-monty": "facade"}.get(m.group(1), m.group(1)), "OUT_DIR/" + m.group(2)
    m = re.search(r"/library/(core|alloc|std|panic_abort|compiler-builtins|stdarch|portable-simd)/", path)
    if m:
        lib = m.group(1)
        if lib == "compiler-builtins":
            return "compiler_builtins", path.split("/library/")[-1]
        return "std/core/alloc", path.split("/library/")[-1]
    m = re.search(r"/dlmalloc-[0-9.]+/(.*)$", path)
    if m:
        return "alloc (dlmalloc, mm-wasm-alloc)", "dlmalloc/" + m.group(1)
    m = re.search(r"/registry/src/[^/]+/([^/]+)/(.*)$", path)
    if m:
        name = re.sub(r"-[0-9][0-9.]*$", "", m.group(1))
        g = {"hashbrown": "std/core/alloc", "compiler_builtins": "compiler_builtins",
             "libm": "compiler_builtins"}.get(name, name)
        return g, m.group(1) + "/" + m.group(2)
    if "/rustc/" in path or "/lib/rustlib/src/" in path:
        return "std/core/alloc", path.split("/library/")[-1]
    return "other", path


def wasm_dwarf(buf):
    from elftools.dwarf.dwarfinfo import DWARFInfo, DebugSectionDescriptor, DwarfConfig
    secs = {}
    code_start = None
    for sid, name, p, e, s in wasm_sections(buf):
        if sid == 10:
            code_start = p
        if sid == 0 and name.startswith(".debug_"):
            n, q = leb(buf, p)
            q += n
            data = buf[q:e]
            secs[name] = DebugSectionDescriptor(io.BytesIO(data), name, None, len(data), 0)
    if ".debug_info" not in secs:
        return None, code_start
    get = secs.get
    di = DWARFInfo(
        config=DwarfConfig(little_endian=True, default_address_size=4, machine_arch="wasm"),
        debug_info_sec=get(".debug_info"), debug_aranges_sec=get(".debug_aranges"),
        debug_abbrev_sec=get(".debug_abbrev"), debug_frame_sec=None, eh_frame_sec=None,
        debug_str_sec=get(".debug_str"), debug_loc_sec=get(".debug_loc"),
        debug_ranges_sec=get(".debug_ranges"), debug_line_sec=get(".debug_line"),
        debug_pubtypes_sec=None, debug_pubnames_sec=None,
        debug_addr_sec=get(".debug_addr"), debug_str_offsets_sec=get(".debug_str_offsets"),
        debug_line_str_sec=get(".debug_line_str"), debug_loclists_sec=get(".debug_loclists"),
        debug_rnglists_sec=get(".debug_rnglists"), debug_sup_sec=None, gnu_debugaltlink_sec=None,
        debug_types_sec=None)
    return di, code_start


# --------------------------------------------------------------------------
# Data attribution


def out_dir_blobs(target_dir, target):
    """{label: bytes} from the build scripts' OUT_DIRs (newest per crate)."""
    blobs = {}
    build = target_dir / target / "release" / "build"
    for crate in ("mm-stdlib", "mm-engine"):
        dirs = sorted((d for d in build.glob(f"{crate}-*/out") if any(d.iterdir())),
                      key=lambda d: d.stat().st_mtime)
        if not dirs:
            continue
        out = dirs[-1]
        for f in sorted(out.glob("*.bin")):
            data = f.read_bytes()
            if len(data) < 16:
                continue
            if crate == "mm-stdlib":
                label = "stdlib header (shared Huffman codes)" if f.stem == "header" else f"stdlib:{f.stem}"
            else:
                label = f"mm-engine:{f.name}"
            blobs[label] = data
    for f in sorted((ROOT / "crates/mm-engine/src/ucd").glob("*.bin")):
        blobs[f"unicode:{f.stem}"] = f.read_bytes()
    return blobs


def classify_bytes(data):
    """Rough kind of an anonymous constant."""
    if not data:
        return "empty"
    printable = sum(1 for b in data if 32 <= b < 127 or b in (9, 10, 13) or b >= 0x80)
    if len(data) >= 4 and printable / len(data) > 0.9:
        return "string constants"
    if not any(data):
        return "zero-initialized"
    return "other constants (tables, vtables, numbers)"


MAP_LINE = re.compile(r"^\s*([0-9a-f]+)\s+[0-9a-f]+\s+([0-9a-f]+)\s+(?:\d+\s+)?\S.*:\((\.[^)]*)\)\s*$")
DATA_PREFIXES = (".rodata.", ".data.rel.ro.local.", ".data.rel.ro.", ".data.", ".bss.", ".tdata.", ".tbss.")


def parse_link_map(path):
    """Data input sections of an lld map (wasm-ld or ELF):
    [(vaddr, size, symbol)] in map order; symbol is a mangled name or
    `anon:<hash>` for the compiler's anonymous constants."""
    out = []
    if not path.exists():
        return out
    for line in path.read_text(errors="replace").splitlines():
        m = MAP_LINE.match(line)
        if not m:
            continue
        vaddr, size, sec = int(m.group(1), 16), int(m.group(2), 16), m.group(3)
        if not size:
            continue
        for pre in DATA_PREFIXES:
            if sec.startswith(pre):
                sym = sec[len(pre):]
                break
        else:
            if sec in (".rodata", ".data.rel.ro", ".data") and "<internal>" in line:
                out.append((vaddr, size, "anon:merged"))
            continue
        if re.fullmatch(r"(cst|str)\d+(\.\d+)?", sym):
            out.append((vaddr, size, "anon:merged"))
            continue
        a = re.match(r"^\.Lanon\.([0-9a-f]+)\.\d+$", sym)
        out.append((vaddr, size, f"anon:{a.group(1)}" if a else sym))
    return out


def map_owners(entries, anon_crate=None, read=None):
    """[(vaddr, size, crate group, label)] for link-map entries. Named
    statics and jump tables get their symbol's crate; anonymous constants
    (strings, tables; label None) the crate of the code that refers to them
    (`anon_crate(vaddr, size)`, wasm only), else "?"."""
    named = [e for e in entries if not e[2].startswith("anon:")]
    info = classify([e[2].removeprefix(".Lswitch.table.") for e in named])
    out = []
    for vaddr, size, sym in entries:
        if sym.startswith("anon:"):
            if read and size > 4096:
                chunk = read(vaddr, size)
                known = next((k for k, sig in KNOWN_ANON.items() if sig in chunk), None)
                if known:
                    out.append((vaddr, size, "mm-engine", known))
                    continue
            dist = anon_crate(vaddr, size) if anon_crate else None
            if not dist:
                out.append((vaddr, size, "?", None))
                continue
            # split by the referring code's crates (merged string pools)
            total, pos = sum(dist.values()), vaddr
            for k, (crate, n) in enumerate(dist.most_common()):
                part = size - (pos - vaddr) if k == len(dist) - 1 else round(size * n / total)
                out.append((pos, part, crate, None))
                pos += part
        else:
            name, g, _ = info[sym.removeprefix(".Lswitch.table.")]
            out.append((vaddr, size, g, name))
    return out


def sleb(buf, pos):
    result = shift = 0
    while True:
        b = buf[pos]
        pos += 1
        result |= (b & 0x7F) << shift
        shift += 7
        if b < 0x80:
            if b & 0x40:
                result -= 1 << shift
            return result, pos


def const_refs(buf, bodies, names, entries):
    """For wasm: anonymous constant -> crate of the functions whose code
    holds its address as an `i32.const` immediate (byte scan, so an
    estimate)."""
    anon = sorted((v, v + s) for v, s, sym in entries if sym.startswith("anon:"))
    starts = [a for a, _ in anon]
    info = classify([names.get(i, f"func[{i}]") for i, _, _ in bodies])
    votes = collections.defaultdict(collections.Counter)
    for idx, lo, hi in bodies:
        g = info[names.get(idx, f"func[{idx}]")][1]
        body = buf[lo:hi]
        pos = body.find(b"\x41")
        while pos >= 0:
            try:
                v, _ = sleb(body, pos + 1)
            except IndexError:
                break
            k = bisect.bisect_right(starts, v) - 1
            if k >= 0 and anon[k][0] <= v < anon[k][1]:
                votes[anon[k][0]][g] += 1
            pos = body.find(b"\x41", pos + 1)
    return lambda vaddr, size: votes.get(vaddr)


# Large anonymous constants recognised by content.
KNOWN_ANON = {
    "builtins.rs PACKED (the whole compile-time table, see NATIVES)": b"printsependfileflush",
}
BLOB_CRATE = {"stdlib": "mm-stdlib", "mm-engine": "mm-engine", "unicode": "mm-engine"}


def attribute_data(segments_bytes, blobs, entries):
    """Owner of every byte of the data segments / sections.

    segments_bytes: [(vaddr, bytes)]; blobs: {label: bytes} found by
    content; entries: map_owners() rows. Returns (Counter{(crate, kind):
    bytes}, {blob label: bytes found}, Counter{(crate, static name): bytes})."""
    # wasm-opt's memory packing drops zero runs between segments, so blobs
    # are searched in the memory image (gaps refilled with zeros).
    segs = sorted(segments_bytes)
    images = []
    for vaddr, data in segs:
        if images and 0 <= vaddr - (images[-1][0] + len(images[-1][1])) < 1 << 16:
            base, img = images[-1]
            img += bytes(vaddr - (base + len(img)))
            img += data
        else:
            images.append((vaddr, bytearray(data)))
    covered = []  # (lo, hi, crate, label)
    found = {}
    for base, img in images:
        for label, blob in blobs.items():
            i = img.find(blob)
            if i >= 0:
                covered.append((base + i, base + i + len(blob), BLOB_CRATE[label.split(":")[0]] if ":" in label else "mm-stdlib", label))
                found[label] = len(blob)
    blob_ranges = sorted(covered)
    for vaddr, size, crate, name in entries:
        lo, hi = vaddr, vaddr + size
        j = bisect.bisect_left(blob_ranges, (hi,))
        # entries that hold a blob (the include_bytes! constant) are the blob
        if any(a < hi and lo < b for a, b, _, _ in blob_ranges[max(0, j - 3):j + 1]):
            continue
        covered.append((lo, hi, crate, name))
    covered.sort()
    owners = collections.Counter()
    statics = collections.Counter()

    def gap(data, crate):
        # split on NUL runs for a better string/table split
        for chunk in re.split(rb"(\x00{4,})", data):
            owners[(crate, classify_bytes(chunk))] += len(chunk)

    starts = [c[0] for c in covered]
    for vaddr, data in segs:
        pos, end = vaddr, vaddr + len(data)
        i = max(0, bisect.bisect_right(starts, vaddr) - 1)
        while i < len(covered) and covered[i][0] < end:
            lo, hi, crate, label = covered[i]
            i += 1
            lo, hi = max(lo, pos), min(hi, end)
            if hi <= lo:
                continue
            if lo > pos:
                gap(data[pos - vaddr:lo - vaddr], "unmapped")
            if label is None:
                gap(data[lo - vaddr:hi - vaddr], crate)
            elif label in blobs or label in KNOWN_ANON:
                owners[(crate, label)] += hi - lo
            else:
                owners[(crate, "named statics and jump tables")] += hi - lo
                statics[(crate, label)] += hi - lo
            pos = hi
        if pos < end:
            gap(data[pos - vaddr:], "unmapped")
    return owners, found, statics


def prelude_sections(target):
    """Packed prelude bytes per `# ---- section` of prelude.py: each unit's
    record (prelude_units.rs) charged its chunk's packed/unpacked ratio."""
    build = TARGET_DIR / target / "release" / "build"
    dirs = sorted((d for d in build.glob("mm-engine-*/out") if (d / "prelude_units.rs").exists()),
                  key=lambda d: d.stat().st_mtime)
    if not dirs:
        return {}
    rs = (dirs[-1] / "prelude_units.rs").read_text()
    header = int(re.search(r"const HEADER: usize = (\d+);", rs).group(1))
    ends = [int(x) for x in re.search(r"static CHUNK_END: \[u32; \d+\] = \[([^\]]*)\]", rs).group(1).split(",")]
    names_lit = re.search(r'static NAMES: &str = "((?:[^"\\]|\\.)*)";', rs).group(1)
    names = names_lit.encode().decode("unicode_escape")
    rows = [tuple(int(v) for v in m.groups())
            for m in re.finditer(r"\[(\d+), (\d+), (\d+), (\d+), (\d+)\]", rs.split("static UNITS")[1])]
    units, start = [], 0
    for end, chunk, at, length, kind in rows:
        units.append((names[start:end], chunk, length, kind))
        start = end
    unpacked = collections.Counter()
    for n, chunk, length, kind in units:
        if kind != 3:
            unpacked[chunk] += length
    packed = {i: ends[i] - (ends[i - 1] if i else header) for i in range(len(ends))}
    # unit -> line -> section
    src = (ROOT / "crates/mm-engine/src/prelude.py").read_text().splitlines()
    section, sec_of = "start", {}
    for i, line in enumerate(src, 1):
        m = re.match(r"^# -{3,} (.+)$", line)
        if m:
            section = m.group(1).strip()
        m = re.match(r"^(?:async )?(?:def|class) (\w+)|^(\w+)(?:\.\w+)? = ", line)
        if m:
            n = m.group(1) or m.group(2)
            if n not in sec_of:
                sec_of[n] = section
    out = collections.Counter({"header (shared Huffman codes)": header})
    for n, chunk, length, kind in units:
        if kind == 3 or not unpacked[chunk]:
            continue
        out[sec_of.get(n, "?")] += length * packed[chunk] / unpacked[chunk]
    return {k: round(v) for k, v in out.most_common()}


# --------------------------------------------------------------------------
# Analysis


def analyze_wasm(label, pre, opt_named, shipped, out, report):
    pre_buf = pre.read_bytes()
    opt_buf = opt_named.read_bytes()
    ship_buf = shipped.read_bytes() if shipped and shipped.exists() else None
    r = report.setdefault(label, {})
    # 1. Equality with the shipped module.
    opt_stripped = strip_custom(opt_buf, keep=lambda n: n not in ("name",) and not n.startswith(".debug"))
    r["named_optimized_bytes"] = len(opt_buf)
    r["named_optimized_without_names"] = len(opt_stripped)
    if ship_buf is not None:
        r["shipped_bytes"] = len(ship_buf)
        r["identical_to_shipped"] = opt_stripped == ship_buf
    secs = wasm_sections(opt_buf)
    r["sections"] = {}
    for sid, name, p, e, s in secs:
        key = SECTION_NAMES.get(sid, f"custom:{name}")
        r["sections"][key] = r["sections"].get(key, 0) + (e - s)
    # 2. Functions of the optimized module.
    _, bodies, names, imported, segments = wasm_functions(opt_buf)
    raw_of = {idx: names.get(idx, f"func[{idx}]") for idx, _, _ in bodies}
    info = classify(list(raw_of.values()))
    funcs = sorted(((hi - lo, raw_of[idx], idx) for idx, lo, hi in bodies), reverse=True)
    # 3. Line tables of the pre-opt module: per function, bytes per file.
    di, code_start = wasm_dwarf(pre_buf)
    _, pre_bodies, pre_names, pre_imported, pre_segments = wasm_functions(pre_buf)
    all_files = collections.Counter()
    pre_sizes = {}
    if di is not None:
        rows = line_rows(di)
        addrs = [a for a, _ in rows]
        for idx, lo, hi in pre_bodies:
            n = pre_names.get(idx, f"func[{idx}]")
            all_files.update(file_bytes(rows, addrs, lo - code_start, hi - code_start))
            pre_sizes[n] = pre_sizes.get(n, 0) + hi - lo
        del rows, addrs
    # 4. Aggregates.
    by_crate = collections.Counter()
    by_module = collections.Counter()
    by_file = collections.Counter()
    unmatched = 0
    for size, name, idx in funcs:
        _, g, mod = info[name]
        by_crate[g] += size
        by_module[f"{g}::{mod}" if mod else g] += size
    # Per file: the pre-wasm-opt module's line tables, scaled by the code
    # wasm-opt kept overall. (Per function scaling would misplace code
    # wasm-opt inlines into its callers, e.g. `dispatch` into `main`.)
    pre_total = sum(pre_sizes.values())
    post_total = sum(s for s, _, _ in funcs)
    by_file_pre = collections.Counter()
    by_user_pre = collections.Counter()
    by_fn_pre = collections.Counter()
    chains_pre = collections.Counter()
    for f, b in all_files.items():
        inner, user, line, frames = f if isinstance(f, tuple) else (f, f, 0, ())
        chains_pre[frames] += b
        by_file_pre[source_group(inner)] += b
        by_user_pre[source_group(user)] += b
        c, fl = source_group(user)
        by_fn_pre[(c, f"{fl}::{rust_fn_at(user, line)}")] += b
    scale = post_total / pre_total if pre_total else 0
    for k, b in by_file_pre.items():
        by_file[k] = b * scale
    r["by_file_user"] = {f"{c} {f}": round(b * scale) for (c, f), b in by_user_pre.most_common()}
    r["by_fn_user"] = {f"{c} {f}": round(b * scale) for (c, f), b in by_fn_pre.most_common() if b * scale >= 20}
    r["chains"] = [[list(k), round(b * scale, 1)] for k, b in chains_pre.items()]
    unmatched = post_total - sum(by_file.values())
    r["code_bytes"] = sum(s for s, _, _ in funcs)
    r["functions"] = len(funcs)
    r["by_crate"] = dict(by_crate.most_common())
    r["by_module"] = dict(by_module.most_common())
    r["by_file"] = {f"{c} {f}": round(b) for (c, f), b in by_file.most_common()}
    r["by_file_pre_wasm_opt"] = {f"{c} {f}": b for (c, f), b in by_file_pre.most_common()}
    r["pre_wasm_opt_code"] = pre_total
    r["code_without_line_match"] = unmatched
    r["top_functions"] = [(s, info[n][0], info[n][1], info[n][2]) for s, n, _ in funcs]
    monos = collections.defaultdict(lambda: [0, 0, 0])
    for size, name, _ in funcs:
        k = mono_key(info[name][0])
        monos[k][0] += size
        monos[k][1] += 1
        monos[k][2] = max(monos[k][2], size)
    r["monos"] = sorted(([k, *v] for k, v in monos.items() if v[1] > 1), key=lambda x: -(x[1] - x[3]))[:80]
    # 5. Data.
    target = UU if "wasip1" not in label else WASI
    blobs = out_dir_blobs(TARGET_DIR, target)
    mapfile = TARGET_DIR / ("guest.map" if "guest" in label else f"hello-{target}.map")
    raw_entries = parse_link_map(mapfile)
    image = {}
    for off, a, b in pre_segments:
        if off is not None:
            image[off] = pre_buf[a:b]

    def read(vaddr, size):
        for off, d in image.items():
            if off <= vaddr < off + len(d):
                return d[vaddr - off:vaddr - off + size]
        return b""

    entries = map_owners(raw_entries, const_refs(pre_buf, pre_bodies, pre_names, raw_entries), read)
    seg_bytes = [(off, opt_buf[a:b]) for off, a, b in segments if off is not None]
    owners, found, statics = attribute_data(seg_bytes, blobs, entries)
    r["data_bytes"] = sum(b - a for _, a, b in segments)
    r["data_section"] = r["sections"].get("data", 0)
    r["data_owners"] = [[c, k, b] for (c, k), b in owners.most_common()]
    r["data_statics"] = [[c, k, b] for (c, k), b in statics.most_common(150)]
    r["blobs_found"] = found
    r["prelude_sections"] = prelude_sections(target)
    r["blobs_missing"] = sorted(set(blobs) - set(found))
    # 6. twiggy.
    tw = tool("TWIGGY", "twiggy") if shutil.which("twiggy") or (Path.home() / ".cargo/bin/twiggy").exists() else None
    if tw:
        d = out / f"twiggy-{label}"
        d.mkdir(parents=True, exist_ok=True)
        for args, fname in ((["top", "-n", "300"], "top.txt"),
                            (["dominators", "-d", "4", "-r", "400"], "dominators.txt"),
                            (["monos", "-m", "60", "-n", "6"], "monos.txt"),
                            (["garbage", "-n", "200"], "garbage.txt"),
                            # paths explodes on broad regexes (25 GB for `BTreeMap.*insert`):
                            # exact names only, shallow.
                            (["paths", "-d", "3", "-r", "20", "--regex",
                              "dragon::format_exact$", "<str>::to_lowercase$"], "paths.txt"),
                            (["top", "-n", "100000", "-f", "csv"], "top.csv")):
            with open(d / fname, "w") as fh:
                subprocess.run([tw, args[0], str(opt_named), *args[1:]], stdout=fh, stderr=subprocess.STDOUT)


def nm_symbols(path):
    out = subprocess.run(["nm", "-S", "--size-sort", "--defined-only", str(path)],
                         check=True, capture_output=True, text=True).stdout
    syms = []
    for line in out.splitlines():
        parts = line.split(None, 3)
        if len(parts) < 4:
            continue
        addr, size, kind, name = parts
        syms.append((int(addr, 16), int(size, 16), kind, name))
    return syms


def elf_sections(path):
    from elftools.elf.elffile import ELFFile
    with open(path, "rb") as fh:
        elf = ELFFile(fh)
        return {s.name: (s["sh_addr"], s["sh_offset"], s["sh_size"]) for s in elf.iter_sections()}


def analyze_native(binary, shipped, out, report):
    from elftools.elf.elffile import ELFFile
    r = report.setdefault("native hello", {})
    secs = elf_sections(binary)
    ship_secs = elf_sections(shipped)
    r["shipped_bytes"] = shipped.stat().st_size
    r["sections"] = {k: v[2] for k, v in ship_secs.items() if v[2]}
    r["identical_sections"] = {k: secs.get(k, (0, 0, -1))[2] == v[2] for k, v in ship_secs.items()
                               if k in (".text", ".rodata", ".data.rel.ro", ".data", ".eh_frame")}
    syms = nm_symbols(binary)
    funcs = sorted(((s, n, a) for a, s, k, n in syms if k in "tTwW"), reverse=True)
    info = classify([n for _, _, _, n in syms])
    with open(binary, "rb") as fh:
        elf = ELFFile(fh)
        di = elf.get_dwarf_info() if elf.has_dwarf_info() else None
        rows = line_rows(di) if di else []
        text = elf.get_section_by_name(".text")
        text_lo, text_hi = text["sh_addr"], text["sh_addr"] + text["sh_size"]
        rodata = elf.get_section_by_name(".rodata")
        ro_addr, ro_bytes = rodata["sh_addr"], rodata.data()
        drr = elf.get_section_by_name(".data.rel.ro")
        drr_addr, drr_bytes = (drr["sh_addr"], drr.data()) if drr else (0, b"")
    by_crate = collections.Counter()
    by_module = collections.Counter()
    for size, name, _ in funcs:
        _, g, mod = info[name]
        by_crate[g] += size
        by_module[f"{g}::{mod}" if mod else g] += size
    by_file = collections.Counter()
    if rows:
        by_user = collections.Counter()
        by_fn = collections.Counter()
        chains = collections.Counter()
        for f, b in file_bytes(rows, [a for a, _ in rows], text_lo, text_hi).items():
            inner, user, line, frames = f if isinstance(f, tuple) else (f, f, 0, ())
            chains[frames] += b
            by_file[source_group(inner)] += b
            by_user[source_group(user)] += b
            c, fl = source_group(user)
            by_fn[(c, f"{fl}::{rust_fn_at(user, line)}")] += b
        r["by_file_user"] = {f"{c} {f}": b for (c, f), b in by_user.most_common()}
        r["by_fn_user"] = {f"{c} {f}": b for (c, f), b in by_fn.most_common() if b >= 20}
        r["chains"] = [[list(k), b] for k, b in chains.items()]
    r["text_bytes"] = text_hi - text_lo
    r["function_symbol_bytes"] = sum(s for s, _, _ in funcs)
    r["functions"] = len(funcs)
    r["by_crate"] = dict(by_crate.most_common())
    r["by_module"] = dict(by_module.most_common())
    r["by_file"] = {f"{c} {f}": b for (c, f), b in by_file.most_common()}
    r["top_functions"] = [(s, info[n][0], info[n][1], info[n][2]) for s, n, _ in funcs]
    monos = collections.defaultdict(lambda: [0, 0, 0])
    for size, name, _ in funcs:
        k = mono_key(info[name][0])
        monos[k][0] += size
        monos[k][1] += 1
        monos[k][2] = max(monos[k][2], size)
    r["monos"] = sorted(([k, *v] for k, v in monos.items() if v[1] > 1), key=lambda x: -(x[1] - x[3]))[:80]
    sections_data = [(ro_addr, ro_bytes), (drr_addr, drr_bytes)]

    def read(vaddr, size):
        for off, d in sections_data:
            if off <= vaddr < off + len(d):
                return d[vaddr - off:vaddr - off + size]
        return b""

    entries = map_owners(parse_link_map(TARGET_DIR / f"hello-{NATIVE}.map"), None, read)
    blobs = out_dir_blobs(TARGET_DIR, NATIVE)
    owners, found, statics = attribute_data([(ro_addr, ro_bytes)], blobs, entries)
    rel, _, rel_statics = attribute_data([(drr_addr, drr_bytes)], {}, entries)
    for (c, k), b in rel.items():
        owners[(c, k if k == "named statics and jump tables" or k in KNOWN_ANON else "pointer tables (.data.rel.ro, relocated at load)")] += b
    statics.update(rel_statics)
    r["rela_dyn_bytes"] = secs.get(".rela.dyn", (0, 0, 0))[2]
    r["data_owners"] = [[c, k, b] for (c, k), b in owners.most_common()]
    r["data_statics"] = [[c, k, b] for (c, k), b in statics.most_common(150)]
    r["blobs_found"] = found
    r["blobs_missing"] = sorted(set(blobs) - set(found))


# --------------------------------------------------------------------------
# Removal builds


# label: (mm-stdlib features dropped from the default set and from every
# other feature's list, features added). Dropping one module alone measures
# its own cost: modules that import it still build (imports resolve at run
# time), so the dependents' bytes stay.
REMOVALS = {
    "-decimal": (["mod-decimal"], []),
    "-asyncio": (["mod-asyncio"], []),
    "-re": (["mod-re"], []),
    "-datetime": (["mod-datetime"], []),
    "-zoneinfo": (["mod-zoneinfo"], []),
    "-json": (["mod-json"], []),
    "-struct": (["mod-struct"], []),
    "-hashlib": (["mod-hashlib"], []),
    "-random": (["mod-random"], []),
    "-binascii": (["mod-binascii"], []),
    "-statistics": (["mod-statistics"], []),
    "-enum": (["mod-enum"], []),
    "-dataclasses": (["mod-dataclasses"], []),
    "-typing": (["mod-typing"], []),
    # mod-struct stays: mm-engine does not build without its `struct`
    # feature (builtins.rs gates `_unpack` but not `_calclen`).
    "no stdlib modules but struct, operator": (["*"], ["mod-struct", "mod-operator"]),
    "+unicodedata": ([], ["mod-unicodedata"]),
    # Not a capability: `cargo xtask size` builds `hello` as an example, so
    # the facade's dev-dependency on itself with `test-hooks` is unified in.
    "facade without test-hooks": ([], []),
}


def removal_builds(out):
    wt = TARGET_DIR / "worktree"
    if not wt.exists():
        run(["git", "worktree", "add", "--detach", str(wt), "HEAD"], cwd=ROOT)
    manifest = wt / "crates/mm-stdlib/Cargo.toml"
    original = (ROOT / "crates/mm-stdlib/Cargo.toml").read_text()
    path_json = out / "removals.json"
    results = json.loads(path_json.read_text()) if path_json.exists() else {}
    td = TARGET_DIR / "removals"
    for label, (drop, add) in [("baseline", ([], []))] + list(REMOVALS.items()):
        text = original
        head, rest = text.split("default = [", 1)
        body, tail = rest.split("]", 1)
        feats = [f.strip().strip('"') for f in body.split(",") if f.strip().strip('"')]
        kept = [f for f in feats if f not in drop and "*" not in drop]
        feats = kept + [a for a in add if a not in kept]
        # A dropped feature must not come back through another's dependency list.
        text = head + "default = [" + ", ".join(f'"{f}"' for f in feats) + "]" + tail
        if drop:
            def fix(line):
                m = re.match(r'^(mod-[\w-]+) = \[(.*)\]$', line)
                if not m:
                    return line
                deps = [d.strip().strip('"') for d in m.group(2).split(",") if d.strip()]
                deps = [d for d in deps if (d not in drop and "*" not in drop) or d in add
                        or (d.startswith("mm-engine/") and m.group(1) in add)]
                return f'{m.group(1)} = [' + ", ".join(f'"{d}"' for d in deps) + "]"
            text = "\n".join(fix(l) for l in text.splitlines()) + "\n"
        manifest.write_text(text)
        facade = wt / "crates/micro-monty/Cargo.toml"
        facade_text = (ROOT / "crates/micro-monty/Cargo.toml").read_text()
        if label == "facade without test-hooks":
            facade.write_text(facade_text.replace('features = ["test-hooks"] }', "}"))
        else:
            facade.write_text(facade_text)
        for target in (UU, NATIVE):
            if target in results.get(label, {}):
                continue
            try:
                path = build_hello(target, root=wt, target_dir=td, named=False)
            except subprocess.CalledProcessError:
                results.setdefault(label, {})[target] = "build failed"
                path_json.write_text(json.dumps(results, indent=1))
                continue
            if target == UU:
                shipped = path.parent.parent / "shipped-removal.wasm"
                wasm_opt(path, shipped, names=False)
                size = shipped.stat().st_size
            else:
                size = path.stat().st_size
            results.setdefault(label, {})[target] = size
            path_json.write_text(json.dumps(results, indent=1))
            print(f"removal {label} {target}: {size}", file=sys.stderr)
    manifest.write_text(original)
    (wt / "crates/micro-monty/Cargo.toml").write_text((ROOT / "crates/micro-monty/Cargo.toml").read_text())


# --------------------------------------------------------------------------
# Capabilities: code whose inline stack has a frame in the capability's
# functions (so helpers inlined into them, and their inlined copies in
# callers, count), plus the data they own.

E = "crates/mm-engine/src/"
CAPABILITIES = [
    # name, frame-key regex, prelude sections, stdlib modules, removal label
    ("parser (mm-parse)", r"^crates/mm-parse/", [], [], None),
    ("compiler (mm-compile compiler*, symtable)", r"^crates/mm-compile/src/(compiler|symtable)", [], [], None),
    ("bytecode encode/decode and packing (mm-compile encode, pack, bytecode)", r"^crates/mm-compile/src/(encode|pack|bytecode)", [], [], None),
    ("GC: mark/sweep (mm-core gc.rs, space sweep, interp collect_*)",
     r"^crates/mm-core/src/gc\.rs|^crates/mm-core/src/space\.rs::(sweep|mark_run_rest)|^" + E + r"interp\.rs::collect_(garbage|now)|^" + E + r"cont\.rs::trace|^crates/mm-core/src/heap\.rs::reserve_marks", [], [], None),
    ("classes: all of class.rs", r"^" + E + r"class\.rs::", ["classes", "metaclasses"], [], None),
    ("  inheritance: C3 MRO, lookup along the MRO, class creation",
     r"^" + E + r"class\.rs::(c3|mro_of|subclass_of|compute_flags|class_changed|user_lookup|class_lookup|own_lookup|meta_of|meta_hook|type_new|build_class|new_class|class_needs_finish)", [], [], None),
    ("  super()", r"^" + E + r"class\.rs::(new_super|super_getattr)|^" + E + r"builtins\.rs::bi_super|^crates/mm-compile/src/compiler\.rs::zero_arg_super", [], [], None),
    ("  descriptors: property, classmethod, staticmethod, __get__/__set__",
     r"^" + E + r"class\.rs::(wrap_descr|bind_attr|bind_class_attr|is_data_descr|no_accessor|native_classmethod)|^" + E + r"builtins\.rs::bi_(property|classmethod|staticmethod)\w*", [], [], None),
    ("dunder dispatch on user objects (dunder.rs)", r"^" + E + r"dunder\.rs::", ["slot wrappers"], [], None),
    ("generators and coroutines (generator.rs, YIELD/AWAIT/SEND arms, compiler genexp/async)",
     r"^" + E + r"generator\.rs::|dispatch(_rare)?\[[^\]]*(YIELD|AWAIT|SEND|GEN|ANEXT|AITER|ASYNC)[^\]]*\]|^crates/mm-compile/src/compiler\.rs::(genexp|anext_step|async_ok)|^" + E + r"interp\.rs::resume", [], [], None),
    ("asyncio (aio.rs, builtins aio_*, prelude asyncio)", r"^" + E + r"aio\.rs::|^" + E + r"builtins\.rs::aio_", ["asyncio"], [], "-asyncio"),
    ("regex (re.rs, re.py)", r"^" + E + r"re\.rs::", [], ["re"], "-re"),
    ("big integers (bigint.rs)", r"^" + E + r"bigint\.rs::", [], [], None),
    ("json (json.rs, prelude json)", r"^" + E + r"json\.rs::", ["json"], [], "-json"),
    ("datetime (datetimemod.rs, datetime, _strptime)", r"^" + E + r"datetimemod\.rs::", [], ["datetime", "datetime._strptime"], "-datetime"),
    ("zoneinfo (Python only)", r"$^", [], ["zoneinfo", "zoneinfo._zoneinfo", "zoneinfo._common", "zoneinfo._tzpath"], "-zoneinfo"),
    ("decimal (Python only)", r"$^", [], ["decimal"], "-decimal"),
    ("snapshots (engine and mm-core snapshot.rs)", r"^" + E + r"snapshot\.rs::|^crates/mm-core/src/snapshot\.rs::", [], [], None),
    ("exceptions and tracebacks (exc.rs)", r"^" + E + r"exc\.rs::", ["reporting"], [], None),
    ("match statement (compiler/matching.rs, matching.rs, parser patterns)",
     r"^crates/mm-compile/src/compiler/matching\.rs|^" + E + r"matching\.rs::|^crates/mm-parse/src/parser\.rs::\w*pattern|dispatch(_rare)?\[[^\]]*MATCH", ["match"], [], None),
    ("hashlib (hashlib.rs, hashlib.py)", r"^" + E + r"hashlib\.rs::", [], ["hashlib"], "-hashlib"),
    ("struct (structmod.rs, struct.py)", r"^" + E + r"structmod\.rs::", [], ["struct"], "-struct"),
    ("random (random.rs, random.py)", r"^" + E + r"random\.rs::", [], ["random"], "-random"),
    ("binascii (binascii.rs, binascii.py)", r"^" + E + r"binascii\.rs::", [], ["binascii"], "-binascii"),
    ("math + libm", r"^" + E + r"(math|libm)\.rs::", [], ["math"], None),
    ("complex + cmath", r"^" + E + r"(complex|cmath)\.rs::", [], [], None),
    ("str methods and formatting (strmeth, percent, text)", r"^" + E + r"(strmeth|percent|text)\.rs::", ["repr"], [], None),
    ("bytes and bytearray", r"^" + E + r"(bytes|bytearray)\.rs::", [], [], None),
    ("host API (host.rs, facade host_defs)", r"^" + E + r"host\.rs::|^crates/micro-monty/src/host_defs", ["host records"], [], None),
    ("unicodedata (opt-in)", r"$^", [], [], "+unicodedata"),
]


def capabilities(v, removals, target):
    chains = v.get("chains", [])
    secs = v.get("prelude_sections", {})
    mods = {k[7:]: b for c, k, b in v.get("data_owners", []) if k.startswith("stdlib:")}
    base = removals.get("baseline", {}).get(target)
    rows = []
    for name, rx, psecs, smods, rem in CAPABILITIES:
        pat = re.compile(rx)
        code = sum(b for frames, b in chains if any(pat.search(f) for f in frames))
        pre = sum(secs.get(p, 0) for p in psecs)
        std = sum(mods.get(m, 0) for m in smods)
        removal = None
        if rem and base and isinstance(removals.get(rem, {}).get(target), int):
            removal = removals[rem][target] - base
        rows.append((name, code, pre, std, removal))
    return rows


# --------------------------------------------------------------------------
# Markdown


ART = [("hello.wasm (wasm32-unknown-unknown)", "hello.wasm uu"),
       ("hello.wasm (wasm32-wasip1)", "hello.wasm wasip1"),
       ("mm_wasm_guest.wasm (wasm32-unknown-unknown)", "guest uu"),
       ("native hello", "native hello")]


def n(x):
    return f"{round(x):,}"


def table(head, rows):
    out = ["| " + " | ".join(head) + " |", "|" + "|".join("---" if i == 0 else "---:" for i in range(len(head))) + "|"]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return "\n".join(out) + "\n"


def file_crate_rollup(v):
    agg = collections.Counter()
    for f, b in v.get("by_file", {}).items():
        c = f.split(" ", 1)[0]
        agg[{"alloc": "dlmalloc + mm-wasm-alloc", "other": "std/core/alloc", "core_arch": "std/core/alloc",
             "core_simd": "std/core/alloc", "unnamed": "wasm-opt outlined"}.get(c, c)] += b
    return agg


def data_rollup(v):
    """Data by owner kind: {kind: bytes}."""
    out = collections.Counter()
    for c, k, b in v.get("data_owners", []):
        if k.startswith("stdlib:") or k.startswith("stdlib header"):
            out["stdlib modules, packed bytecode"] += b
        elif k.startswith("mm-engine:prelude"):
            out["prelude, packed bytecode"] += b
        elif k.startswith("unicode:"):
            out[f"unicode table {k[8:]}.bin"] += b
        elif k == "string constants":
            out[f"string constants ({c})"] += b
        elif k == "named statics and jump tables":
            out[f"named statics, jump tables ({c})"] += b
        else:
            out[f"{k} ({c})"] += b
    return out


def markdown(report, removals):
    md = []
    md.append("## Totals\n")
    rows = []
    for key, short in ART:
        v = report.get(key)
        if not v:
            continue
        secs = v["sections"]
        if key.startswith("native"):
            code = secs.get(".text", 0)
            data = secs.get(".rodata", 0) + secs.get(".data.rel.ro", 0) + secs.get(".data", 0)
            other = v["shipped_bytes"] - code - data
        else:
            code, data = secs.get("code", 0), secs.get("data", 0)
            other = v["named_optimized_without_names"] - code - data
        total = v.get("shipped_bytes") or v["named_optimized_without_names"]
        rows.append([short, n(total), n(v.get("named_optimized_without_names", total)), n(code), n(data), n(other)])
    md.append(table(["build", "shipped", "named build w/o names", "code", "data", "other"], rows))
    md.append("\n## Code by crate (line tables, inlined code counted where written)\n")
    roll = {short: file_crate_rollup(report[key]) for key, short in ART if key in report}
    crates = sorted({c for r in roll.values() for c in r}, key=lambda c: -roll.get("hello.wasm uu", {}).get(c, 0))
    md.append(table(["crate"] + list(roll), [[c] + [n(roll[s].get(c, 0)) for s in roll] for c in crates]))
    md.append("\n## Code by crate (function names; where each out-of-line function is defined)\n")
    byname = {short: report[key]["by_crate"] for key, short in ART if key in report}
    crates = sorted({c for r in byname.values() for c in r}, key=lambda c: -byname.get("hello.wasm uu", {}).get(c, 0))
    md.append(table(["crate"] + list(byname), [[c] + [n(byname[s].get(c, 0)) for s in byname] for c in crates if max(byname[s].get(c, 0) for s in byname) >= 500]))
    md.append("\n## Code by source file (repository crates)\n")
    files = {short: report[key]["by_file"] for key, short in ART if key in report}
    allf = sorted({f for r in files.values() for f in r}, key=lambda f: -files.get("hello.wasm uu", {}).get(f, 0))
    md.append(table(["file"] + list(files), [[f] + [n(files[s].get(f, 0)) for s in files] for f in allf if max(files[s].get(f, 0) for s in files) >= 1500]))
    md.append("\n## Code by source file, mm-core's inlined helpers charged to their callers\n")
    files = {short: report[key].get("by_file_user", {}) for key, short in ART if key in report}
    allf = sorted({f for r in files.values() for f in r}, key=lambda f: -files.get("hello.wasm uu", {}).get(f, 0))
    md.append(table(["file"] + list(files), [[f] + [n(files[s].get(f, 0)) for s in files] for f in allf if max(files[s].get(f, 0) for s in files) >= 1500]))
    md.append("\n## Capabilities\n")
    w = capabilities(report["hello.wasm (wasm32-unknown-unknown)"], removals, UU)
    nt = capabilities(report["native hello"], removals, NATIVE)
    rows = []
    for (name, code, pre, std, rem), (_, ncode, _, _, nrem) in zip(w, nt):
        rows.append([name, n(code), n(ncode), n(pre), n(std), n(code + pre + std),
                     "" if rem is None else f"{rem:+,}", "" if nrem is None else f"{nrem:+,}"])
    md.append(table(["capability", "code wasm", "code native", "prelude", "stdlib bytecode",
                     "wasm total", "removal wasm", "removal native"], rows))
    md.append("\n## Data by owner\n")
    dr = {short: data_rollup(report[key]) for key, short in ART if key in report}
    kinds = sorted({k for r in dr.values() for k in r}, key=lambda k: -max(r.get(k, 0) for r in dr.values()))
    md.append(table(["owner"] + list(dr), [[k] + [n(dr[s].get(k, 0)) for s in dr] for k in kinds if max(dr[s].get(k, 0) for s in dr) >= 200]))
    v = report["hello.wasm (wasm32-unknown-unknown)"]
    md.append("\n## Bundled stdlib modules, packed (all builds identical)\n")
    mods = [(k[7:], b) for c, k, b in v["data_owners"] if k.startswith("stdlib:")]
    mods.sort(key=lambda x: -x[1])
    md.append(table(["module", "bytes"], [[m, n(b)] for m, b in mods]))
    md.append("\n## Prelude by section (packed bytes, chunk ratio applied per unit)\n")
    md.append(table(["section", "bytes"], [[k, n(b)] for k, b in v.get("prelude_sections", {}).items()]))
    md.append("\n## Named statics (hello.wasm uu, top 40)\n")
    md.append(table(["crate", "static", "bytes"], [[c, k[:110].replace("|", "\\|"), n(b)] for c, k, b in v["data_statics"][:40]]))
    for key, short in (("hello.wasm (wasm32-unknown-unknown)", "hello.wasm uu"), ("mm_wasm_guest.wasm (wasm32-unknown-unknown)", "guest uu"), ("native hello", "native hello")):
        md.append(f"\n## Top 60 functions, {short}\n")
        md.append(table(["#", "bytes", "crate", "function"],
                        [[i + 1, n(s), g, f[:140].replace("|", "\\|")] for i, (s, f, g, m) in enumerate(report[key]["top_functions"][:60])]))
    for key, short in (("hello.wasm (wasm32-unknown-unknown)", "hello.wasm uu"), ("native hello", "native hello")):
        md.append(f"\n## Monomorphized families, {short} (bytes beyond the largest copy)\n")
        md.append(table(["generic", "copies", "total", "largest", "beyond largest"],
                        [[k[:110].replace("|", "\\|"), c, n(t), n(mx), n(t - mx)] for k, t, c, mx in report[key]["monos"][:40]]))
    if removals:
        md.append("\n## Removal builds (as shipped, stripped)\n")
        base = removals.get("baseline", {})
        rows = []
        for label, v in removals.items():
            rows.append([label] + [(f"{n(v[t])} ({v[t] - base.get(t, v[t]):+,})" if isinstance(v[t], int) else v[t]) if t in v else "" for t in (UU, NATIVE)])
        md.append(table(["build", "hello.wasm uu", "native hello"], rows))
    return "\n".join(md)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--skip-build", action="store_true")
    ap.add_argument("--removals", action="store_true")
    ap.add_argument("--only-removals", action="store_true")
    ap.add_argument("--markdown", action="store_true", help="only render size-map.md from the JSON")
    ap.add_argument("--out", default=str(TARGET_DIR / "report"))
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    report_path = out / "size-map.json"
    report = json.loads(report_path.read_text()) if report_path.exists() else {}
    if not args.only_removals and not args.markdown:
        artifacts = {}
        if not args.skip_build:
            build_hello(UU)
            build_guest()
            build_hello(WASI)
            build_hello(NATIVE)
        rel = lambda t: TARGET_DIR / t / "release"
        artifacts["hello.wasm (wasm32-unknown-unknown)"] = (rel(UU) / "examples/hello.wasm",
                                                          ROOT / "target" / UU / "release/shipped/hello.wasm")
        artifacts["mm_wasm_guest.wasm (wasm32-unknown-unknown)"] = (rel(UU) / "mm_wasm_guest.wasm",
                                                                  ROOT / "target" / UU / "release/shipped/mm_wasm_guest.wasm")
        artifacts["hello.wasm (wasm32-wasip1)"] = (rel(WASI) / "examples/hello.wasm",
                                                  ROOT / "target" / WASI / "release/shipped/hello.wasm")
        for label, (pre, shipped) in artifacts.items():
            stem = re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-")
            nodwarf = out / f"{stem}.pre.wasm"
            nodwarf.write_bytes(strip_custom(pre.read_bytes(), keep=lambda n: not n.startswith(".debug")))
            named = out / f"{stem}.named.wasm"
            wasm_opt(nodwarf, named, names=True)
            analyze_wasm(label, pre, named, shipped, out, report)
            ship_pre = shipped.parent.parent / ("examples/hello.wasm" if "hello" in label else "mm_wasm_guest.wasm")
            if ship_pre.exists():
                sizes = lambda b: {SECTION_NAMES[x[0]]: x[3] - x[2] for x in wasm_sections(b) if x[0] in (10, 11)}
                report[label]["pre_wasm_opt_sections_named_vs_shipped"] = [
                    sizes(nodwarf.read_bytes()), sizes(ship_pre.read_bytes())]
        analyze_native(rel(NATIVE) / "examples/hello", ROOT / "target" / NATIVE / "release/examples/hello",
                       out, report)
        report_path.write_text(json.dumps(report, indent=1))
    if (args.removals or args.only_removals) and not args.markdown:
        removal_builds(out)
    removals_path = out / "removals.json"
    removals = json.loads(removals_path.read_text()) if removals_path.exists() else {}
    if report:
        (out / "size-map.md").write_text(markdown(report, removals))
    print(f"wrote {report_path} and size-map.md", file=sys.stderr)


if __name__ == "__main__":
    main()

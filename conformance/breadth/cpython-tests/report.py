#!/usr/bin/env python3.14
"""Summarize harness.py output into results/.

Usage: python3.14 -I report.py RAW_DIR [--out results]

RAW_DIR is harness.py's --out (it holds units/*.json). Writes:
  methods.jsonl  one line per method: unit, id, cpython status, mm status,
                 mm category, mm exception type, message, test source line
  units.jsonl    one line per unit: counts, stubs, chunking, process attempts
  summary.md     per-unit table and failure causes by number of methods
"""

import argparse
import ast
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True
sys.path.insert(0, HERE)
import harness  # noqa: E402

PASS = ("pass", "xfail")

DELIBERATE_NAMES = {"eval", "exec", "compile", "globals"}
# Left out by design per docs/spec.md §1/§4 and docs/user-guide.md "Known differences from CPython":
# no ambient I/O, no threads/weakref, no exec/eval, no __slots__/__del__,
# inspect, difflib.HtmlDiff and struct.pack_into not bundled. CPython C-API
# test modules count here too.
DELIBERATE_MODULES = {"threading", "_thread", "weakref", "subprocess", "socket", "ctypes",
                      "multiprocessing", "sqlite3", "signal", "inspect", "tempfile", "shutil",
                      "_testcapi", "_testinternalcapi", "_testlimitedcapi", "_testsinglephase"}
DELIBERATE_ATTRS = {"difflib.HtmlDiff", "struct.pack_into"}
DELIBERATE_SYNTAX = ("__slots__", "__del__", "t-strings")
AMBIENT_IO = ("has no ambient filesystem", "has no ambient operating system")


def def_lines(lib_test, unit):
    """Top-level name -> 'relpath:line' of the statement binding it."""
    b = harness.Builder(lib_test)
    path = b.resolve(unit)
    with open(path, encoding="utf-8") as f:
        tree = ast.parse(f.read())
    rel = os.path.relpath(path, lib_test)
    out = {}
    for s in tree.body:
        for n in harness.bound_names([s]):
            out.setdefault(n, "%s:%d" % (rel, harness.stmt_start(s)))
    return out


def cause(m, unit, ctx, depth=0):
    """(category, root cause key) for an mm non-pass on a CPython pass. A
    NameError for a top-level name whose defining statement raised at
    import is attributed to that statement's error."""
    st, typ, msg = m["status"], m.get("type", ""), m.get("msg", "")
    if typ == "NameError" and depth < 6:
        x = re.search(r"name '([^']+)' is not defined", msg)
        if x and x.group(1) in ctx["defs"]:
            where = ctx["defs"][x.group(1)]
            for t in ctx["tops"]:
                if str(t[0]) == where:
                    return cause({"status": "error", "type": t[1], "msg": t[2]}, unit, ctx, depth + 1)
            return "module-level statement failed", "top-level %s not created" % x.group(1)
    if st in ("load", "crash", "timeout"):
        if typ == "CompileError":
            return "process: compile", "module does not compile on mm: " + msg[:120]
        if st == "timeout":
            return "process: timeout", "timeout (%s)" % typ
        first = msg.splitlines()[-1] if msg else ""
        return "process: " + st, "%s: %s" % (st, re.sub(r"0x[0-9a-f]+|\d+", "N", first)[:120])
    if st == "skip":
        return "skipped on mm", "skipped on mm only"
    if typ == "_MMExcluded":
        x = re.search(r"does not support (.+)$", msg)
        what = x.group(1) if x else msg
        cat = "excluded: compile (deliberate)" if any(d in what for d in DELIBERATE_SYNTAX) \
            else "excluded: compile (unsupported syntax)"
        return cat, "compile-time: micro-monty does not support " + what
    if typ in ("ModuleNotFoundError", "_MMMissingModuleError", "ImportError"):
        x = re.search(r"No module named '([^']+)'", msg) or re.search(r"cannot import name '([^']+)' from '([^']+)'", msg)
        if x and x.lastindex == 2:
            return "missing module/attribute", "missing: %s.%s" % (x.group(2), x.group(1))
        name = x.group(1) if x else msg[:80]
        base = name.split(".")[0]
        cat = "missing module (deliberate)" if base in DELIBERATE_MODULES else "missing module"
        return cat, "missing module: " + name
    if typ == "NameError":
        x = re.search(r"name '([^']+)' is not defined", msg)
        if x:
            n = x.group(1)
            if n in DELIBERATE_NAMES:
                return "missing builtin (deliberate)", "builtin %s() not defined (deliberate)" % n
            return "missing builtin", "name %s not defined" % n
    if typ == "AttributeError":
        x = re.search(r"^(?:type object|module)? ?'([^']+)'(?: object)? has no attribute '([^']+)'", msg)
        if x:
            key = "%s.%s" % (x.group(1), x.group(2))
            return ("missing attribute (deliberate)" if key in DELIBERATE_ATTRS else "missing attribute"), \
                "missing attribute: " + key
    if any(a in msg for a in AMBIENT_IO):
        return "no ambient I/O (deliberate)", "%s: %s" % (typ, msg.split(":")[0][:80])
    if typ == "_MMShimMissing":
        return "shim gap on mm", msg[:120]
    if st == "fail":
        return "wrong result", "assertion failed in " + unit
    norm = re.sub(r"0x[0-9a-f]+", "ADDR", msg)
    norm = re.sub(r"\d+", "N", norm)
    return "wrong exception/error", "%s: %s" % (typ, norm[:120])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("raw")
    ap.add_argument("--lib-test", required=True)
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "results"))
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    order = []
    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "modules.txt")) as f:
        for ln in f:
            ln = ln.split("#", 1)[0].strip()
            if ln:
                order.append(ln)
    recs = {}
    for u in order:
        p = os.path.join(args.raw, "units", u + ".json")
        if os.path.exists(p):
            with open(p) as f:
                recs[u] = json.load(f)
    methods_out = []
    units_out = []
    rows = []
    causes = collections.defaultdict(lambda: {"n": 0, "units": collections.Counter(), "cat": "", "ex": None})
    totals = collections.Counter()
    for u in order:
        r = recs.get(u)
        if r is None:
            rows.append((u, 0, 0, 0, "not run", ""))
            continue
        ms = r.get("methods", {})
        ctx = {"defs": def_lines(args.lib_test, u), "tops": r.get("mm_toplevel_errors") or []}
        cp_pass = mm_pass = cp_skip = 0
        cats = collections.Counter()
        for tid in sorted(ms):
            v = ms[tid]
            c = v.get("cpython") or {"status": "missing"}
            m = v.get("mm") or {"status": "missing"}
            row = {"unit": u, "id": tid, "cpython": c["status"], "mm": m["status"]}
            if c["status"] in PASS:
                cp_pass += 1
                if m["status"] in PASS:
                    mm_pass += 1
                else:
                    cat, key = cause(m, u, ctx)
                    cats[cat] += 1
                    row.update({"category": cat, "cause": key, "type": m.get("type", ""),
                                "msg": m.get("msg", "")[:300], "where": m.get("where", "")})
                    cz = causes[key]
                    cz["n"] += 1
                    cz["units"][u] += 1
                    cz["cat"] = cat
                    if cz["ex"] is None:
                        cz["ex"] = (u, tid, m.get("where", ""), m.get("msg", "")[:160])
            elif c["status"] == "skip":
                cp_skip += 1
            methods_out.append(row)
        totals["total"] += len(ms)
        totals["cp"] += cp_pass
        totals["mm"] += mm_pass
        totals["cp_skip"] += cp_skip
        note = r.get("fatal") or "; ".join(r.get("mm_fatal") or [])
        rows.append((u, len(ms), cp_pass, mm_pass, note,
                     ", ".join("%s %d" % (k, n) for k, n in cats.most_common(4))))
        units_out.append({"unit": u, "total": len(ms), "cpython_pass": cp_pass, "cpython_skip": cp_skip,
                          "mm_pass": mm_pass, "fatal": r.get("fatal"), "mm_fatal": r.get("mm_fatal"),
                          "mm_stubs": r.get("mm_stubs", []), "mm_chunking": r.get("mm_chunking"),
                          "mm_toplevel_errors": r.get("mm_toplevel_errors", []),
                          "cpython_toplevel_errors": r.get("cpython_toplevel_errors", []),
                          "stubbed_imports": (r.get("info") or {}).get("stubbed_imports"),
                          "helpers": (r.get("info") or {}).get("helpers"),
                          "notes": (r.get("info") or {}).get("notes"),
                          "cpython_attempts": r.get("cpython_attempts"), "mm_attempts": r.get("mm_attempts"),
                          "seconds": r.get("seconds")})
    with open(os.path.join(args.out, "methods.jsonl"), "w") as f:
        for row in methods_out:
            f.write(json.dumps(row, sort_keys=True) + "\n")
    with open(os.path.join(args.out, "units.jsonl"), "w") as f:
        for row in units_out:
            f.write(json.dumps(row, sort_keys=True) + "\n")
    with open(os.path.join(args.out, "summary.md"), "w") as f:
        f.write("# CPython 3.14.7 Lib/test on micro-monty: results\n\n")
        f.write("Generated by `report.py`. A method counts for mm only when it passes on CPython "
                "with the same shim (pass or expected failure).\n\n")
        pct = 100.0 * totals["mm"] / totals["cp"] if totals["cp"] else 0
        f.write("Overall: %d methods, %d pass on CPython (%d skipped there), %d of those pass on mm "
                "(%.1f%%).\n\n" % (totals["total"], totals["cp"], totals["cp_skip"], totals["mm"], pct))
        f.write("| unit | methods | pass CPython | pass mm | mm % | mm failure categories (methods) |\n")
        f.write("|---|---:|---:|---:|---:|---|\n")
        for u, t, c, m, note, cats in rows:
            p = "%.0f" % (100.0 * m / c) if c else "-"
            extra = cats + ((" — " + note.replace("|", "/").replace("\n", " ")[:160]) if note else "")
            f.write("| %s | %d | %d | %d | %s | %s |\n" % (u, t, c, m, p, extra))
        f.write("\n## Failure causes by number of affected methods\n\n")
        f.write("Methods that pass on CPython and not on mm, grouped by the first exception the "
                "mm run reported (`cause()` in report.py).\n\n")
        f.write("| methods | category | cause | units (top 3) | example |\n|---:|---|---|---|---|\n")
        for key, cz in sorted(causes.items(), key=lambda kv: -kv[1]["n"]):
            ex = cz["ex"]
            f.write("| %d | %s | %s | %s | `%s` %s |\n" % (
                cz["n"], cz["cat"], key.replace("|", "/"),
                ", ".join("%s %d" % (k, n) for k, n in cz["units"].most_common(3)),
                ex[1], ex[2]))
    print("total %d  cpython %d  mm %d  (%.1f%%)" % (totals["total"], totals["cp"], totals["mm"], pct))


if __name__ == "__main__":
    main()

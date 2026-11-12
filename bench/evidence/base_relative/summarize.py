"""Tables for docs/research/base-relative-snapshots.md from run.py's output.

    python3.14 -I bench/evidence/base_relative/summarize.py OUT_DIR > OUT_DIR/summary.md
    python3.14 -I bench/evidence/base_relative/summarize.py --timing PINNED_OUT_DIR...
"""
import json
import statistics
import sys
from pathlib import Path

if sys.argv[1] != "--timing":
    out = Path(sys.argv[1])
    images = [json.loads(l) for l in (out / "images.jsonl").read_text().splitlines()]
    checks = [json.loads(l) for l in (out / "checks.jsonl").read_text().splitlines()]


def med(xs):
    return statistics.median(xs) if xs else float("nan")


def groups():
    by = {}
    for r in images:
        by.setdefault(r["image"], []).append(r)
    for rs in by.values():
        rs.sort(key=lambda r: r["park"])
    g = {}
    own = [rs[0] for n, rs in by.items() if n.startswith("own/")]
    g["own cases, first park"] = own
    cm = {n: rs for n, rs in by.items() if n.startswith("codemode/")}
    g["codemode, first park"] = [rs[0] for rs in cm.values()]
    g["codemode, last park"] = [rs[-1] for rs in cm.values()]
    g["codemode, every park"] = [r for rs in cm.values() for r in rs]
    bench = {n: rs for n, rs in by.items() if n.startswith("bench/")}
    g["bench, middle park"] = [rs[len(rs) // 2] for rs in bench.values()]
    for n, rs in sorted(bench.items()):
        g[f"{n} (middle park)"] = [rs[len(rs) // 2]]
    for n in ["state/1000", "state/10000", "state/100000"]:
        if n in by:
            g[f"{n} rows"] = [by[n][0]]
    return g


def share(r):
    return r["heap_same_bytes"] / r["full_bytes"]


def report():
    print("## Sizes (medians; [M])\n")
    print("| Images | n | FULL | heap same as base | segs same | Δb64 | Δb256 | Δb4k | Δseg | "
          "FULL+zstd1 | FULL+zstd1 (base prefix) | Δb64+zstd1 | Δb256+zstd1 |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for name, rs in groups().items():
        if not rs:
            continue
        f = lambda k: med([r[k] for r in rs])  # noqa: E731
        d = lambda v: med([r["delta_bytes"][v] for r in rs])  # noqa: E731
        z = lambda k: med([r[k]["bytes"] for r in rs])  # noqa: E731
        segs = f"{sum(r['segments_identical'] for r in rs)}/{sum(r['segments'] for r in rs)}"
        print(f"| {name} | {len(rs)} | {f('full_bytes'):,.0f} | {med([r['heap_same_bytes'] for r in rs]):,.0f} "
              f"({med([share(r) for r in rs]):.0%}) | {segs} | {d('b64'):,.0f} | {d('b256'):,.0f} | "
              f"{d('b4k'):,.0f} | {d('seg'):,.0f} | {z('full_zstd1'):,.0f} | {z('full_zstd1_prefix'):,.0f} | "
              f"{z('b64_zstd1'):,.0f} | {z('b256_zstd1'):,.0f} |")

    print("\n## Ratios against FULL raw (median of per-image ratios; [M])\n")
    print("| Images | Δb64 | Δb256 | FULL+zstd1 | FULL+zstd1 (base prefix) | Δb64+zstd1 | Δb256+zstd1 | "
          "prefix vs FULL+zstd1 | Δb64+zstd1 vs FULL+zstd1 |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for name, rs in groups().items():
        if not rs:
            continue
        q = lambda fn: med([r["full_bytes"] / fn(r) for r in rs])  # noqa: E731
        vs = lambda fn: med([r["full_zstd1"]["bytes"] / fn(r) for r in rs])  # noqa: E731
        print(f"| {name} | {q(lambda r: r['delta_bytes']['b64']):.1f}× | {q(lambda r: r['delta_bytes']['b256']):.1f}× | "
              f"{q(lambda r: r['full_zstd1']['bytes']):.1f}× | {q(lambda r: r['full_zstd1_prefix']['bytes']):.1f}× | "
              f"{q(lambda r: r['b64_zstd1']['bytes']):.1f}× | {q(lambda r: r['b256_zstd1']['bytes']):.1f}× | "
              f"{vs(lambda r: r['full_zstd1_prefix']['bytes']):.2f}× | {vs(lambda r: r['b64_zstd1']['bytes']):.2f}× |")

    print("\n## Where the bytes are (medians; [M])\n")
    print("| Images | FULL | header | VM section | heap live | heap live same as base | base live |")
    print("|---|---:|---:|---:|---:|---:|---:|")
    for name, rs in groups().items():
        if not rs:
            continue
        f = lambda k: med([r[k] for r in rs])  # noqa: E731
        print(f"| {name} | {f('full_bytes'):,.0f} | {f('prefix_bytes'):,.0f} | {f('vm_bytes'):,.0f} | "
              f"{f('heap_live_bytes'):,.0f} | {f('heap_same_bytes'):,.0f} | {f('base_live_bytes'):,.0f} |")

    print("\n## Behaviour ([M])\n")
    own = [c for c in checks if c["name"].startswith(("own/", "bench/", "state/"))]
    cm = [c for c in checks if c["name"].startswith("codemode/")]
    print(f"- program runs (own, bench, state): {len(own)}; swap output == plain output: "
          f"{sum(1 for c in own if c.get('same'))}; parks restored from base + delta: "
          f"{sum(c.get('parks') or 0 for c in own if c.get('same')):,}; bench checksum lines "
          f"matched: {sum(1 for c in own if c.get('expected'))}/{sum(1 for c in own if 'expected' in c)}")
    print(f"- codemode solutions: {len(cm)}; measured run == runner: "
          f"{sum(1 for c in cm if c['same_measured'])}; swap run == runner: "
          f"{sum(1 for c in cm if c['same_swap'])}; host calls: {sum(c['calls'] for c in cm):,}; "
          f"parks measured: {sum(1 for r in images if r['image'].startswith('codemode/')):,}")
    bad = [c for c in checks if not (c.get("same", True) and c.get("same_swap", True)
                                      and c.get("same_measured", True) and c.get("expected", True))]
    for c in bad:
        print(f"  - differs: `{json.dumps(c)[:300]}`")


def timing(dirs):
    """Save and restore times of the pinned runs (`taskset -c 2`, one job)."""
    print("| Image | FULL | save: FULL+zstd1 | save: base prefix | save: Δb64+zstd1 | "
          "restore: FULL+zstd1 | restore: base prefix | restore: Δb64+zstd1 |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|")
    for d in dirs:
        rows = [json.loads(l) for l in (Path(d) / "images.jsonl").read_text().splitlines()]
        by = {}
        for r in rows:
            by.setdefault(r["image"], []).append(r)
        for name, rs in sorted(by.items()):
            rs.sort(key=lambda r: r["park"])
            r = rs[0] if len(rs) < 3 else rs[len(rs) // 2]
            ms = lambda ns: f"{ns / 1e6:.2f}"  # noqa: E731
            snap, rest = r["snapshot_ns"], r["restore_ns"]
            enc, app = r["delta_ns"]["b64"]["encode_ns"], r["delta_ns"]["b64"]["apply_ns"]
            z, p, b = r["full_zstd1"], r["full_zstd1_prefix"], r["b64_zstd1"]
            print(f"| {name} (park {r['park']}) | {r['full_bytes']:,} | "
                  f"{ms(snap)} + {ms(z['encode_ns'])} | {ms(snap)} + {ms(p['encode_ns'])} | "
                  f"{ms(snap)} + {ms(enc)} + {ms(b['encode_ns'])} | "
                  f"{ms(z['decode_ns'])} + {ms(rest)} | {ms(p['decode_ns'])} + {ms(rest)} | "
                  f"{ms(b['decode_ns'])} + {ms(app)} + {ms(rest)} |")


if __name__ == "__main__":
    if sys.argv[1] == "--timing":
        timing(sys.argv[2:])
    else:
        report()

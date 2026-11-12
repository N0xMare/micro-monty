"""Session templates against Session::new plus imports: Ir, time, RSS, image size.

    python3.14 -I bench/evidence/session_templates/measure.py OUT_DIR [--sets a,b] [--reps N]

Needs `tpl` (this directory, built into target/session-templates, or `$TPL`) and valgrind.
For each set it writes the template image, then measures one operation,
`fresh` (Session::new, the zone source, the import cell) or `restore`
(Engine::restore of the image):

- Ir: callgrind instruction counts of `tpl ir MODE SET N` for N = 0, 1 and
  11: `first` = Ir(1) - Ir(0) (includes one-time process work such as the
  first unpacking of bundled code), `steady` = (Ir(11) - Ir(1)) / 10;
- wall: `tpl time` under `taskset -c 2`, first operation in a new process,
  then median and min of REPS;
- RSS: `tpl rss` holding N sessions, (after - before) / N (`--rss-n`: one
  or more N, default 50, 200 for `empty`);
- size: the image raw, zstd-1, and zstd-1 with the startup image (the `empty`
  template) as a raw prefix (docs/sdk.md "Storing snapshots"); zstd decode
  time best of 5.

`--module-cache BYTES` sets the engine's module cache limit for every run
(`TPL_MODULE_CACHE`; 0 is off, the default is `Engine::new`'s).

Writes OUT_DIR/templates.json and prints a Markdown table.
"""
import argparse
import concurrent.futures
import json
import os
import re
import subprocess
import time
from pathlib import Path

from compression import zstd

ROOT = Path(__file__).resolve().parents[3]
TPL = Path(os.environ.get("TPL", ROOT / "target/session-templates/release/tpl"))
SETS = ["empty", "json", "codemode", "codemode-tz", "heavy"]


def ir(mode, s, n, image):
    cmd = ["valgrind", "--tool=callgrind", "--callgrind-out-file=/dev/null", str(TPL), "ir", mode, s,
           str(n)] + ([str(image)] if mode == "restore" else [])
    p = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return int(re.search(r"Collected : (\d+)", p.stderr).group(1))


def tpl(*args, pin=False):
    cmd = (["taskset", "-c", "2"] if pin else []) + [str(TPL), *map(str, args)]
    return json.loads(subprocess.run(cmd, capture_output=True, text=True, check=True).stdout)


def best(f, n=5):
    t = []
    for _ in range(n):
        s = time.perf_counter_ns()
        out = f()
        t.append(time.perf_counter_ns() - s)
    return out, min(t)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", type=Path)
    ap.add_argument("--sets", default=",".join(SETS))
    ap.add_argument("--reps", type=int, default=51)
    ap.add_argument("--rss-n", default="")
    ap.add_argument("--module-cache")
    a = ap.parse_args()
    if a.module_cache is not None:
        os.environ["TPL_MODULE_CACHE"] = a.module_cache
    a.out.mkdir(parents=True, exist_ok=True)
    sets = a.sets.split(",")
    for s in set(sets) | {"empty"}:
        subprocess.run([TPL, "image", s, a.out / f"{s}.mmsnap"], check=True, capture_output=True)
    prefix = zstd.ZstdDict((a.out / "empty.mmsnap").read_bytes(), is_raw=True).as_prefix
    rows = {s: {} for s in sets}
    jobs = [(s, m, n) for s in sets for m in ("fresh", "restore") for n in (0, 1, 11)]
    with concurrent.futures.ThreadPoolExecutor(12) as pool:
        counts = dict(zip(jobs, pool.map(lambda j: ir(j[1], j[0], j[2], a.out / f"{j[0]}.mmsnap"), jobs)))
    for s in sets:
        image = (a.out / f"{s}.mmsnap").read_bytes()
        r = rows[s]
        r["image_bytes"] = len(image)
        z, _ = best(lambda: zstd.compress(image, 1))
        zp, _ = best(lambda: zstd.compress(image, 1, zstd_dict=prefix))
        _, dec = best(lambda: zstd.decompress(zp, zstd_dict=prefix))
        r["zstd1_bytes"], r["zstd1_prefix_bytes"], r["zstd1_prefix_decode_ns"] = len(z), len(zp), dec
        for m in ("fresh", "restore"):
            c = {n: counts[(s, m, n)] for n in (0, 1, 11)}
            r[f"{m}_ir_first"] = c[1] - c[0]
            r[f"{m}_ir_steady"] = (c[11] - c[1]) // 10
            r[f"{m}_time"] = tpl("time", m, s, a.reps, a.out / f"{s}.mmsnap", pin=True)
            ns = [int(n) for n in a.rss_n.split(",") if n] or [50 if s != "empty" else 200]
            for n in ns:
                rss = tpl("rss", m, s, n, a.out / f"{s}.mmsnap")
                r[f"{m}_rss_kib_per_session_n{n}"] = (rss["after_kib"] - rss["before_kib"]) / n
                r[f"{m}_module_cache_bytes"] = rss.get("module_cache_bytes")
            r[f"{m}_rss_kib_per_session"] = r[f"{m}_rss_kib_per_session_n{ns[-1]}"]
    (a.out / "templates.json").write_text(json.dumps(rows, indent=1) + "\n")
    print("| Set | fresh Ir (steady / first) | restore Ir (steady / first) | fresh µs (median / first) "
          "| restore µs (median / first) | image | zstd-1 | zstd-1, startup prefix | RSS KiB/session fresh / restore |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for s, r in rows.items():
        f, t = r["fresh_time"], r["restore_time"]
        print(f"| {s} | {r['fresh_ir_steady'] / 1e6:.2f}M / {r['fresh_ir_first'] / 1e6:.2f}M "
              f"| {r['restore_ir_steady'] / 1e6:.2f}M / {r['restore_ir_first'] / 1e6:.2f}M "
              f"| {f['median_ns'] / 1e3:,.0f} / {f['first_ns'] / 1e3:,.0f} "
              f"| {t['median_ns'] / 1e3:,.0f} / {t['first_ns'] / 1e3:,.0f} "
              f"| {r['image_bytes']:,} | {r['zstd1_bytes']:,} | {r['zstd1_prefix_bytes']:,} "
              f"| {r['fresh_rss_kib_per_session']:,.0f} / {r['restore_rss_kib_per_session']:,.0f} |")


if __name__ == "__main__":
    main()

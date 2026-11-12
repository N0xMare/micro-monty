"""Base-relative snapshots: sizes, times and behaviour (docs/research/base-relative-snapshots.md).

    python3.14 -I bench/evidence/base_relative/run.py OUT_DIR [--jobs N] [--filter S]

Needs `br` (this directory, built into target/base-relative) and the codemode
`full` runner (target/codemode/full). Workloads:

- own: every conformance/own case, first park at `--every 100` (as the
  2026-11-04 compression study), then a swap run parked every 100 fuel units
  and restored from base + delta at each park, compared with a plain run;
- bench: every bench/*.py parked about 12 times through its run, every park
  measured, swap run compared with the plain run and bench/expected.txt;
- state: 1k/10k/100k rows of four-field dicts, parked in a loop after
  building them (the compression study's programs, made to finish);
- codemode: every solution, every host call and wait measured, a swap run
  compared with the unchanged runner's outcome, calls, batches and stdout.

zstd is level 1 (python 3.14 `compression.zstd`), single-threaded, best of 5;
`zstd1_prefix` uses the base image as a raw-content prefix (zstd's
`--patch-from`), which needs no engine change.
"""
import argparse
import concurrent.futures
import json
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from compression import zstd

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
BR = ROOT / "target/base-relative/release/br"
CODEMODE = ROOT / "target/codemode/full/release/mm-codemode"
VARIANTS = ["b64", "b256", "b1k", "b4k", "seg"]
STATE = """data = [{{'i': i, 's': 'tag', 'f': i * 0.5, 'k': str(i)}} for i in range({rows})]
total = 0
while total < 12_000_000:
    total += 1
print(len(data), total, sum(r['i'] for r in data))
"""


def best(f, data, n=5):
    t = []
    for _ in range(n):
        s = time.perf_counter_ns()
        out = f(data)
        t.append(time.perf_counter_ns() - s)
    return out, min(t)


def zstd_row(base: bytes, prefix, d: Path, park: int) -> dict:
    full = (d / f"{park}.full").read_bytes()
    row = {}
    packed, enc = best(lambda b: zstd.compress(b, 1), full)
    _, dec = best(zstd.decompress, packed)
    row["full_zstd1"] = dict(bytes=len(packed), encode_ns=enc, decode_ns=dec)
    packed, enc = best(lambda b: zstd.compress(b, 1, zstd_dict=prefix), full)
    out, dec = best(lambda b: zstd.decompress(b, zstd_dict=prefix), packed)
    assert out == full
    row["full_zstd1_prefix"] = dict(bytes=len(packed), encode_ns=enc, decode_ns=dec)
    for v in VARIANTS:
        delta = (d / f"{park}.{v}").read_bytes()
        packed, enc = best(lambda b: zstd.compress(b, 1), delta)
        _, dec = best(zstd.decompress, packed)
        row[f"{v}_zstd1"] = dict(bytes=len(packed), encode_ns=enc, decode_ns=dec)
    return row


KEEP = None


def analyse(metrics: Path, d: Path, base: bytes, prefix, keep=None) -> list[dict]:
    rows = [json.loads(l) for l in metrics.read_text().splitlines()] if metrics.exists() else []
    if keep:
        rows = keep(rows)
    for r in rows:
        r.update(zstd_row(base, prefix, d, r["park"]))
        if KEEP:
            dest = KEEP / r["image"].replace("/", "__")
            dest.mkdir(parents=True, exist_ok=True)
            shutil.copy(d / f"{r['park']}.full", dest / f"{r['park']}.full")
    return rows


def br(args, **kw):
    return subprocess.run([str(BR), *map(str, args)], capture_output=True, text=True,
                          timeout=900, **kw)


def program_case(name, path, every, base_path, base, prefix, measure, keep=None, expect=None):
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        metrics = tmp / "m.jsonl"
        common = [path, "--base", base_path, "--every", every, "--name", name]
        m = br(["program", *common, "--measure", measure, "--dump", tmp],
               env={"BR_METRICS": str(metrics)})
        if m.returncode:
            return {"name": name, "error": m.stderr}, []
        plain = br(["program", *common])
        swap = br(["program", *common, "--swap"])
        check = {"name": name, "plain": plain.returncode, "swap": swap.returncode,
                 "same": plain.returncode == 0 and plain.stdout == swap.stdout,
                 "parks": json.loads(plain.stdout)["parks"] if plain.returncode == 0 else None}
        if expect is not None and plain.returncode == 0:
            check["expected"] = expect in json.loads(swap.stdout)["stdout"]
        if swap.returncode:
            check["swap_error"] = swap.stderr[-400:]
        return check, analyse(metrics, tmp, base, prefix, keep)


def codemode_case(item, base_path, base, prefix):
    sys.path.insert(0, str(ROOT / "conformance/codemode"))
    import run as cm  # noqa: E402
    set_name, task_id, path = item
    source = path.read_text()
    task = cm.ptc.TASKS[task_id]
    mode = cm.SETS[set_name]
    name = f"codemode/{set_name}/{task_id}"
    keys = ("outcome", "error", "problem", "calls", "batches", "stdout")
    ref = cm.run_one([str(CODEMODE)], task, mode, source)
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        metrics = tmp / "m.jsonl"
        cmd = [str(BR), "codemode", "--base", str(base_path), "--name", name]
        measured = cm.run_one(cmd + ["--measure", "all", "--dump", str(tmp),
                                     "--metrics", str(metrics)], task, mode, source)
        swap = cm.run_one(cmd + ["--swap"], task, mode, source)
        check = {"name": name,
                 "same_measured": all(ref[k] == measured[k] for k in keys),
                 "same_swap": all(ref[k] == swap[k] for k in keys),
                 "outcome": ref["outcome"], "calls": len(ref["calls"])}
        return check, analyse(metrics, tmp, base, prefix)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", type=Path)
    ap.add_argument("--jobs", type=int, default=8)
    ap.add_argument("--filter", default="")
    ap.add_argument("--only", default="own,bench,state,codemode")
    ap.add_argument("--warm", help="base = fresh session after running this code")
    ap.add_argument("--keep", type=Path, help="keep every measured FULL image here (dictionary.py)")
    args = ap.parse_args()
    out = args.out
    out.mkdir(parents=True, exist_ok=True)
    base_path = out / "base.mmsnap"
    subprocess.run([BR, "base", base_path, *([args.warm] if args.warm else [])], check=True)
    global KEEP
    KEEP = args.keep
    base = base_path.read_bytes()
    prefix = zstd.ZstdDict(base, is_raw=True).as_prefix
    only = args.only.split(",")
    images, checks = [], []

    def collect(results):
        for check, rows in results:
            checks.append(check)
            images.extend(rows)
            if not (check.get("same", True) and check.get("same_swap", True)
                    and check.get("same_measured", True) and check.get("expected", True)
                    and "error" not in check):
                print("MISMATCH", json.dumps(check)[:600], flush=True)

    pool = concurrent.futures.ThreadPoolExecutor(args.jobs)
    if "own" in only:
        cases = [p for p in sorted((ROOT / "conformance/own").glob("*.py")) if args.filter in p.name]
        collect(pool.map(lambda p: program_case(f"own/{p.name}", p, 100, base_path, base, prefix,
                                                 "first"), cases))
        print(f"own: {len(cases)} cases", flush=True)
    if "bench" in only:
        expected = dict(l.split(": ", 1) for l in (ROOT / "bench/expected.txt").read_text().splitlines()
                        if l and not l.startswith("#"))

        def bench(p):
            probe = br(["program", p, "--base", base_path, "--every", 10_000])
            parks = json.loads(probe.stdout)["parks"]
            every = max(1_000, parks * 10_000 // 12)
            name = p.stem if p.stem != "json_rows" else "json"
            return program_case(f"bench/{p.stem}", p, every, base_path, base, prefix, "all",
                                expect=f"{expected[name]}")
        progs = [p for p in sorted((ROOT / "bench").glob("*.py")) if args.filter in p.name]
        collect(pool.map(bench, progs))
        print(f"bench: {len(progs)} programs", flush=True)
    if "state" in only:
        with tempfile.TemporaryDirectory() as tmp:
            def state(rows):
                p = Path(tmp, f"state{rows}.py")
                p.write_text(STATE.format(rows=rows))
                return program_case(f"state/{rows}", p, 2_000_000, base_path, base, prefix,
                                    "first")
            collect(map(state, [1000, 10000, 100000]))
        print("state: 3 programs", flush=True)
    if "codemode" in only:
        sys.path.insert(0, str(ROOT / "conformance/codemode"))
        import run as cm
        items = [i for i in cm.solutions(None) if args.filter in f"{i[0]}/{i[1]}"]
        collect(pool.map(lambda i: codemode_case(i, base_path, base, prefix), items))
        print(f"codemode: {len(items)} solutions", flush=True)
    with (out / "images.jsonl").open("w") as f:
        for r in images:
            f.write(json.dumps(r) + "\n")
    with (out / "checks.jsonl").open("w") as f:
        for c in checks:
            f.write(json.dumps(c) + "\n")
    print(f"{len(images)} images, {len(checks)} runs written to {out}")


if __name__ == "__main__":
    main()

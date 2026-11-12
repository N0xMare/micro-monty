"""Host-module definition cost: Session::new + define, snapshots, restore.

    python3.14 -I bench/evidence/host_defs/measure.py OUT.json [--hd PATH] [--reps N]

Needs `hd` (built into target/host-defs, or `--hd`), valgrind and zstd. For
10, 100 and 600 typed tools in one module (and 1,200 in 12 modules):
wall ns of `Session::new` plus `define`, median of REPS under
`taskset -c 2`, on a new Engine each time (cold) and on one that defined
them before (warm), and the first one in a new process (median of 7); steady Ir of the same, (Ir(11) - Ir(1)) / 10; RSS per
session holding 40 (10 for 600 tools) warm sessions; the image with the
tools, raw and zstd -1 with a new session's image as the prefix
(`--patch-from`); Engine::restore of it, cold and warm; and the same
image from an Engine that stores host code in images.
"""
import argparse
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SETS = [(10, 1), (100, 1), (600, 1), (1200, 12)]


def hd(exe, *args, pin=False):
    cmd = (["taskset", "-c", "2"] if pin else []) + [exe, *map(str, args)]
    return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout


def ir(exe, *args):
    cmd = ["valgrind", "--tool=callgrind", "--callgrind-out-file=/dev/null", exe, *map(str, args)]
    p = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return int(re.search(r"Collected : (\d+)", p.stderr).group(1))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--hd", default=str(ROOT / "target/host-defs/release/hd"))
    ap.add_argument("--reps", type=int, default=21)
    a = ap.parse_args()
    res = {}
    for tools, mods in SETS:
        r = res[f"{tools}/{mods}"] = {}
        for mode in ("cold", "warm"):
            r[f"{mode}_ns"] = json.loads(hd(a.hd, "time", tools, mods, a.reps, mode, pin=True))["median_ns"]
            r[f"{mode}_ir"] = (ir(a.hd, "ir", tools, mods, 11, mode) - ir(a.hd, "ir", tools, mods, 1, mode)) / 10
        # The first define in a new process on a new Engine, the median over
        # 7 processes: cold_ns's later sessions run beside the earlier
        # Engines, which keep their caches, so their memory is never reused.
        firsts = sorted(json.loads(hd(a.hd, "time", tools, mods, 1, "cold", pin=True))["first_ns"] for _ in range(7))
        r["cold_process_ns"] = firsts[3]
        n = 10 if tools >= 600 else 40
        rss = json.loads(hd(a.hd, "rss", tools, mods, n))
        r["rss_kib_per_session"] = (rss["after_kib"] - rss["before_kib"]) / n
        with tempfile.TemporaryDirectory() as d:
            hd(a.hd, "snap", tools, mods, d)
            base, image = Path(d, "base.mmsnap"), Path(d, "tools.mmsnap")
            r["image_bytes"] = image.stat().st_size
            r["base_bytes"] = base.stat().st_size
            z = subprocess.run(["zstd", "-1", "-q", "-c", f"--patch-from={base}", str(image)],
                               capture_output=True, check=True).stdout
            r["image_zstd1_prefix_bytes"] = len(z)
            z = subprocess.run(["zstd", "-1", "-q", "-c", str(image)], capture_output=True, check=True).stdout
            r["image_zstd1_bytes"] = len(z)
            for mode in ("cold", "warm"):
                t = json.loads(hd(a.hd, "restore", a.reps, mode, image, pin=True))
                r[f"restore_{mode}_ns"] = t["median_ns"]
            # Host code stored in the image (Engine::with_host_code_in_snapshots;
            # builds before it ignore `stored` and always store it).
            hd(a.hd, "snap", tools, mods, d, "stored")
            r["stored_image_bytes"] = image.stat().st_size
            z = subprocess.run(["zstd", "-1", "-q", "-c", f"--patch-from={base}", str(image)],
                               capture_output=True, check=True).stdout
            r["stored_image_zstd1_prefix_bytes"] = len(z)
            t = json.loads(hd(a.hd, "restore", a.reps, "cold", image, pin=True))
            r["stored_restore_cold_ns"] = t["median_ns"]
        print(tools, mods, r, flush=True)
    Path(a.out).write_text(json.dumps(res, indent=1) + "\n")


if __name__ == "__main__":
    main()

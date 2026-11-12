"""The codemode suite started from session templates instead of fresh sessions.

    python3.14 -I bench/evidence/session_templates/codemode.py OUT_DIR TEMPLATE=IMAGE... [--jobs N] [--skip-check] [--warm]

Uses the `default` mm-codemode runner (target/codemode/default, run.py's
build) and its `MM_CODEMODE_TEMPLATE` option; each TEMPLATE=IMAGE is a
`tpl image` of the same build.

1. Behaviour: every solution (run.py's sets, probes included) runs fresh and
   from each template; outcome, error, checker verdict, tool calls, batches
   and stdout must be identical (checks.jsonl).
2. Timing: the census solutions (no probes), one at a time under
   `taskset -c 2`, through conformance/codemode/timing.py's harness; the
   runner's own counters give setup (session or restore, host module,
   bootstrap cell) and engine (`start`/`resume`) time per run (timing.jsonl).
   `--warm` times the second setup in each process (`MM_CODEMODE_WARM`), as
   a long-lived host would see it; without it, setup is a new process's
   first.
"""
import argparse
import concurrent.futures
import json
import statistics
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "conformance/codemode"))
import ptc  # noqa: E402
import run  # noqa: E402
import timing  # noqa: E402

RUNNER = ROOT / "target/codemode/default/release/mm-codemode"
KEYS = ("outcome", "error", "problem", "calls", "batches", "stdout")


WARM = False


def command(image):
    env = (["MM_CODEMODE_WARM=1"] if WARM else []) + ([f"MM_CODEMODE_TEMPLATE={image}"] if image else [])
    return (["env", *env] if env else []) + [str(RUNNER)]


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(p * len(xs)))]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", type=Path)
    ap.add_argument("templates", nargs="+")
    ap.add_argument("--jobs", type=int, default=14)
    ap.add_argument("--skip-check", action="store_true")
    ap.add_argument("--warm", action="store_true", help="time a second setup in each process")
    a = ap.parse_args()
    global WARM
    WARM = a.warm
    a.out.mkdir(parents=True, exist_ok=True)
    configs = {"fresh": None} | dict(t.split("=", 1) for t in a.templates)
    items = run.solutions(None)

    if not a.skip_check:
        def check(item):
            set_name, task_id, path = item
            source, task, mode = path.read_text(), ptc.TASKS[task_id], run.SETS[set_name]
            ref = run.run_one(command(None), task, mode, source)
            rec = {"id": f"{set_name}/{task_id}", "outcome": ref["outcome"]}
            for name, image in configs.items():
                if image is not None:
                    got = run.run_one(command(image), task, mode, source)
                    rec[name] = all(ref[k] == got[k] for k in KEYS)
            return rec
        with concurrent.futures.ThreadPoolExecutor(a.jobs) as pool:
            recs = list(pool.map(check, items))
        with (a.out / "checks.jsonl").open("w") as f:
            for r in recs:
                f.write(json.dumps(r) + "\n")
        for name in configs:
            if configs[name] is not None:
                same = sum(r[name] for r in recs)
                print(f"behaviour, {name}: {same} of {len(recs)} solutions identical to fresh")
                for r in recs:
                    if not r[name]:
                        print(f"  differs: {r['id']}")

    census = [i for i in items if i[0] != "p"]
    rows = []
    for set_name, task_id, path in census:
        source, task, mode = path.read_text(), ptc.TASKS[task_id], run.SETS[set_name]
        rec = {"id": f"{set_name}/{task_id}"}
        for name, image in configs.items():
            t = timing.timed(["taskset", "-c", "2", *command(image)], task, mode, source)
            rec[name] = {k: t.get(k) for k in ("setup", "compile", "engine", "start", "ok", "calls")}
        rows.append(rec)
    with (a.out / "timing.jsonl").open("w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")
    ok = [r for r in rows if all(r[c]["ok"] for c in configs)]
    print(f"\ntiming: {len(ok)} of {len(rows)} census solutions without an error in every configuration\n")
    print("| Cells | config | engine p50 / p90 / p99 ms | setup p50 ms | setup + engine p50 / p90 / p99 ms |")
    print("|---|---|---:|---:|---:|")
    for label, sel in (("all", ok), ("dt-*", [r for r in ok if r["id"].split("/")[1].startswith("dt-")])):
        for c in configs:
            e = [r[c]["engine"] * 1e3 for r in sel]
            s = [r[c]["setup"] * 1e3 for r in sel]
            se = [x + y for x, y in zip(e, s)]
            print(f"| {label} ({len(sel)}) | {c} | {pct(e, .5):.3f} / {pct(e, .9):.3f} / {pct(e, .99):.3f} "
                  f"| {statistics.median(s):.3f} | {pct(se, .5):.3f} / {pct(se, .9):.3f} / {pct(se, .99):.3f} |")


if __name__ == "__main__":
    main()

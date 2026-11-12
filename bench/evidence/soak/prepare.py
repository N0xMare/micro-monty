"""The soak harness's workload file (bench/evidence/soak/README.md).

    python3.14 -I bench/evidence/soak/prepare.py OUT.jsonl [--runner PATH] [--jobs N]

One JSON object per line:

- `{"kind": "codemode", "id", "source", "tools", "clock", "replies"}`: every
  codemode solution (conformance/codemode/solutions, probes included), with
  the tool replies of one run through the `full` mm-codemode runner and the
  suite's mocked tools (`replies[i]` answers the i-th host call:
  `{"name", "ok"}` or `{"name", "err": [type, message]}`). The soak replays
  them in order in-process, so it needs no Python tool host.
- `{"kind": "own", "id", "source", "file"}`: every conformance/own case, run
  as `mm run` runs it (`__name__ == '__main__'`, `__file__` set).

The soak adds one import item per bundled module itself (from
`Engine::environment`). The runner defaults to
`target/codemode/full/release/mm-codemode` (built here when missing, as
conformance/codemode/run.py builds it).
"""
import argparse
import concurrent.futures
import json
import subprocess
import sys
import threading
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "conformance/codemode"))
import ptc  # noqa: E402
import run  # noqa: E402

_local = threading.local()


class RecordingHost(ptc.ToolHost):
    """ToolHost that keeps every wire reply, in call order."""

    def __init__(self, task, mode):
        super().__init__(task, mode)
        self.replies = []
        _local.host = self

    def handle(self, name, args, kwargs):
        reply = super().handle(name, args, kwargs)
        self.replies.append({"name": name, **reply})
        return reply


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", type=Path)
    ap.add_argument("--runner", type=Path, default=ROOT / "target/codemode/full/release/mm-codemode")
    ap.add_argument("--jobs", type=int, default=8)
    a = ap.parse_args()
    if not a.runner.exists():
        subprocess.run(["cargo", "build", "--release", "--locked", "--quiet", "-p", "mm-codemode",
                        "--target-dir", str(ROOT / "target/codemode/full"), "--features", "optional-modules"],
                       cwd=ROOT, check=True)
    run.ptc.ToolHost = RecordingHost

    def record(item):
        set_name, task_id, path = item
        source, task, mode = path.read_text(), ptc.TASKS[task_id], run.SETS[set_name]
        result = run.run_one([str(a.runner)], task, mode, source)
        return {"kind": "codemode", "id": f"{set_name}/{task_id}", "source": source,
                "tools": [t.name for t in task.tool_defs()], "clock": run.CLOCK,
                "replies": _local.host.replies, "outcome": result["outcome"]}

    with concurrent.futures.ThreadPoolExecutor(a.jobs) as pool:
        codemode = list(pool.map(record, run.solutions(None)))
    own = [{"kind": "own", "id": f"own/{p.name}", "source": p.read_text(), "file": str(p)}
           for p in sorted((ROOT / "conformance/own").glob("*.py"))]
    with a.out.open("w") as f:
        for rec in codemode + own:
            f.write(json.dumps(rec) + "\n")
    outcomes = {}
    for r in codemode:
        outcomes[r["outcome"]] = outcomes.get(r["outcome"], 0) + 1
    print(f"{len(codemode)} codemode solutions {outcomes}, "
          f"{sum(len(r['replies']) for r in codemode)} recorded replies; {len(own)} own cases -> {a.out}")


if __name__ == "__main__":
    main()

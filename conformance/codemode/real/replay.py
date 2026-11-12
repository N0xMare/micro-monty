#!/usr/bin/env python3
"""Replays recorded sessions on the other worker and compares every cell.

    python3 replay.py RAW_DIR OUT_JSONL --monty WORKER --mm WORKER

For each session recorded by `run_sessions.py`, it starts the worker the
session did NOT use, sends the same `Configure` and every `StartCell` in
order (so globals carry over as they did), and answers the k-th nested call
of a cell with the recorded result of that cell's k-th call. A call whose
name or input differs from the recording stops the cell's replay as
`diverged`. Each cell gets one verdict:

- `same`: same outcome kind and the same output text and final value;
- `same outcome, other traceback`: both failed with the same output and
  the same last line, but different traceback text (caret style);
- `same outcome, other message`: both failed with the same output but a
  different exception line (repr, wording);
- `same outcome, other output`: both completed (or both failed) but the
  printed text or final value differs;
- `only original failed` / `only replay failed`: the decisive cases;
- `diverged`: the replay made a different host call;
- `skipped`: the recording had a `Cancel` or yields this replay does not
  reproduce.

Outputs that depend on the clock or entropy differ legitimately; review
those by hand.
"""

import argparse
import json
import struct
import subprocess
import sys
from pathlib import Path

MM_IDENTITY = ("micro-monty", "micro-monty-cell-adapter.v1")


class Worker:
    def __init__(self, path):
        self.process = subprocess.Popen(
            [path], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, env={"PATH": "/usr/bin:/bin"})

    def send(self, frame):
        body = json.dumps(frame).encode()
        self.process.stdin.write(struct.pack(">I", len(body)) + body)
        self.process.stdin.flush()

    def recv(self):
        head = self.process.stdout.read(4)
        if len(head) < 4:
            return None
        (length,) = struct.unpack(">I", head)
        return json.loads(self.process.stdout.read(length))

    def close(self):
        try:
            self.send({"frame": "shutdown"})
            self.process.stdin.close()
            self.process.wait(timeout=5)
        except Exception:
            self.process.kill()


def recorded_cells(trace):
    """Per cell: its start frame, its recorded calls with results, output, outcome."""
    cells, by_id = [], {}
    configure = None
    for line in trace.open():
        item = json.loads(line)
        frame = item.get("f")
        if not frame:
            continue
        name = frame.get("frame")
        if name == "configure":
            configure = frame
        elif name == "reset":
            cells.append({"reset": True})
        elif name == "start_cell":
            cell = {"start": frame, "calls": [], "results": {}, "output": "",
                    "outcome": None, "skip": False}
            cells.append(cell)
            by_id[frame["cell"]] = cell
        elif "cell" in frame and frame["cell"] in by_id:
            cell = by_id[frame["cell"]]
            if name == "nested_call":
                cell["calls"].append(frame)
            elif name == "nested_result":
                cell["results"][frame["call"]] = frame["outcome"]
            elif name == "output":
                cell["output"] += frame["item"].get("text", "")
            elif name == "cell_done":
                cell["outcome"] = frame["outcome"]
            elif name in ("cancel", "resume", "yielded"):
                cell["skip"] = True
    return configure, cells


def kind(outcome):
    return (outcome or {}).get("outcome", "lost")


def replay_cell(worker, cell):
    start = cell["start"]
    worker.send(start)
    output, k = "", 0
    while True:
        frame = worker.recv()
        if frame is None:
            return {"outcome": "lost"}, output, None
        name = frame.get("frame")
        if name == "host_lost":
            return {"outcome": "lost", "message": frame["message"]}, output, None
        if name == "output":
            output += frame["item"].get("text", "")
        elif name == "nested_call":
            if k >= len(cell["calls"]):
                return None, output, f"extra call {frame['name']}"
            want = cell["calls"][k]
            if (want["name"], want["input"]) != (frame["name"], frame["input"]):
                return None, output, f"call {k}: {frame['name']} vs recorded {want['name']}"
            frame["_answer"] = cell["results"].get(want["call"], {"Err": "no recorded result"})
            cell.setdefault("_pending", {})[frame["call"]] = frame["_answer"]
            k += 1
        elif name == "waiting":
            for call in frame["calls"]:
                answer = cell.get("_pending", {}).pop(call, None)
                if answer is None:
                    return None, output, f"waiting on unknown call {call}"
                worker.send({"frame": "nested_result", "cell": start["cell"],
                             "call": call, "outcome": answer})
        elif name == "cell_done":
            return frame["outcome"], output, None


def verdict(original, replayed, original_output, replayed_output):
    a, b = kind(original), kind(replayed)
    if (a == "completed") != (b == "completed"):
        return "only original failed" if b == "completed" else "only replay failed"
    same_text = original_output == replayed_output and \
        (original or {}).get("value") == (replayed or {}).get("value")
    if a == b and same_text:
        left, right = (original or {}).get("message"), (replayed or {}).get("message")
        if left != right:
            last = lambda m: (m or "").strip().splitlines()[-1:]
            if last(left) == last(right):
                return "same outcome, other traceback"
            return "same outcome, other message"
        return "same"
    return "same outcome, other output"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("raw")
    parser.add_argument("out")
    parser.add_argument("--monty", required=True)
    parser.add_argument("--mm", required=True)
    args = parser.parse_args()
    with open(args.out, "w") as out:
        for run_file in sorted(Path(args.raw).glob("*/*/run.json")):
            run = json.loads(run_file.read_text())
            other = args.monty if run["worker"] == "mm" else args.mm
            for trace in sorted(run_file.parent.glob("trace-*.jsonl")):
                configure, cells = recorded_cells(trace)
                if configure is None:
                    continue
                worker = Worker(other)
                hello = worker.recv()
                worker.send(configure)
                broken = None
                for cell in cells:
                    if cell.get("reset"):
                        worker.send({"frame": "reset"})
                        continue
                    record = {"arm": run["arm"], "prompt": run["prompt"],
                              "cell": cell["start"]["cell"], "replayed_on": other,
                              "replay_hello": hello and hello.get("engine")}
                    if broken or cell["skip"] or cell["outcome"] is None:
                        record["verdict"] = "skipped"
                        record["why"] = broken or ("unfinished" if cell["outcome"] is None else "cancel or yield")
                    else:
                        replayed, text, diverged = replay_cell(worker, cell)
                        if diverged:
                            record["verdict"] = "diverged"
                            record["why"] = diverged
                            broken = "an earlier cell diverged"
                            # The worker still holds the diverged cell.
                        else:
                            record["verdict"] = verdict(cell["outcome"], replayed, cell["output"], text)
                            record["original"] = {"outcome": kind(cell["outcome"]),
                                                  "message": (cell["outcome"] or {}).get("message"),
                                                  "output": cell["output"][:2000]}
                            record["replay"] = {"outcome": kind(replayed),
                                                "message": (replayed or {}).get("message"),
                                                "output": text[:2000]}
                            if kind(replayed) == "lost":
                                broken = "the replay worker was lost"
                    out.write(json.dumps(record) + "\n")
                    out.flush()
                worker.close()
            print(f"replayed {run['arm']} {run['prompt']}", file=sys.stderr)


if __name__ == "__main__":
    main()

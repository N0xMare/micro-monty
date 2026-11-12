#!/usr/bin/env python3
"""Turns raw `run_sessions.py` output into per-cell records and a summary.

    python3 census.py RAW_DIR OUT_DIR [--replay REPLAY_JSONL] [--machine TEXT]

Writes OUT_DIR/cells.jsonl (one record per cell), OUT_DIR/sessions.jsonl
(one per session) and OUT_DIR/summary.md. Times come from the tracer's
monotonic clock; memory from the worker's /proc at each cell's end.
`engine_ms` is the cell's wall time minus the time it waited on host calls
(from each `waiting` frame to the last result of its batch). plak's QuickJS
worker sends no `waiting` frames, so `engine_ms_calls` is the one measure for
both: wall time minus the union of the intervals in which a nested call is
outstanding (its `nested_call` to its `nested_result`).
"""

import ast
import json
import re
import statistics
import sys
import warnings
from collections import Counter, defaultdict
from pathlib import Path


def imports_of(source):
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            tree = ast.parse(source)
    except SyntaxError as error:
        return None, None, f"SyntaxError: {error.msg}"
    modules, names = set(), set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            modules.update(alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.ImportFrom) and node.module:
            modules.add(node.module.split(".")[0])
            names.update(f"{node.module}.{alias.name}" for alias in node.names)
    return sorted(modules), sorted(names), None


def first_cause(message):
    if message.startswith("type errors; nothing ran"):
        codes = sorted(set(re.findall(r"error\[([a-z-]+)\]", message)))
        return "type check: " + ", ".join(codes)
    lines = [l for l in message.strip().splitlines() if l.strip()]
    return lines[-1].strip()[:200] if lines else ""


def classify(record):
    """A first guess; `replay.py` and review decide the rest."""
    if record["outcome"] == "completed":
        return None
    cause = record["cause"]
    if record["outcome"] == "lost":
        return "worker lost"
    if record["outcome"] == "stopped":
        return "foreground budget"
    if cause.startswith("type check:"):
        return "type check (nothing ran)"
    source = record["source"]
    if record["composition"].startswith("js-"):
        return classify_js(record, cause, source)
    javascript = ("console.log" in source or re.search(r"\bconst \w+ =", source)
                  or "=>" in source or "JSON.parse" in source)
    if javascript and ("SyntaxError" in cause or "NameError" in cause):
        return "model: JavaScript"
    if "NameError" in cause and re.search(r"\{\s*%s\s*:" % re.escape(
            cause.split("'")[1] if cause.count("'") >= 2 else "\0"), source):
        return "model: JavaScript"
    if record["cpython_syntax"] and "SyntaxError" in cause:
        return "model: syntax error"
    for marker in ("not supported", "micro-monty", "NotImplementedError",
                   "ModuleNotFoundError", "not implemented", "cannot type-check"):
        if marker in cause:
            return "engine: refusal or gap"
    if cause.startswith(("RuntimeError", "ValueError", "OverflowError")) and \
            record["host_calls"]:
        return "host: tool error"
    return "model: runtime error"


PYTHON_IN_JS = re.compile(
    r"^\s*(def |import \w|from \S+ import |elif |for \w+ in |while True:|try:|except)|\bprint\(|"
    r"\b(True|False|None)\b|\blen\(|\.append\(|\bf\"|\bnot in\b|\bstr\(|\bint\(", re.M)
NODE_GLOBALS = ("Buffer", "require", "process", "__dirname", "fs", "module", "global")
JS_STRINGS = re.compile(r"`(?:\\.|[^`\\])*`|\"(?:\\.|[^\"\\\n])*\"|'(?:\\.|[^'\\\n])*'", re.S)


def classify_js(record, cause, source):
    """As `classify`, for js-ptc cells on QuickJS. Python is looked for outside
    string literals, so a Python script passed to a terminal tool is not one."""
    undefined = re.match(r"ReferenceError: '?([\w$]+)'? is not defined", cause)
    if undefined and undefined.group(1) in NODE_GLOBALS:
        return "sandbox: no Node API"
    code = JS_STRINGS.sub('""', source)
    if re.match(r"(SyntaxError|ReferenceError)", cause) and PYTHON_IN_JS.search(code):
        return "model: Python"
    if cause.startswith("SyntaxError"):
        return "model: syntax error"
    if record["host_calls"] and any(h["ok"] is False for h in record["host_calls"]):
        return "host: tool error"
    return "model: runtime error"


def union_ms(intervals):
    total, end = 0.0, None
    for start, stop in sorted(intervals):
        if end is None or start > end:
            total += stop - start
            end = stop
        elif stop > end:
            total += stop - end
            end = stop
    return total


def max_overlap(intervals):
    """Most nested calls outstanding at once (both workers; QuickJS has no `waiting`)."""
    events = sorted([(t0, 1) for t0, _ in intervals] + [(t1, -1) for _, t1 in intervals])
    peak = depth = 0
    for _, step in events:
        depth += step
        peak = max(peak, depth)
    return peak


def parse_trace(path, run):
    frames = [json.loads(line) for line in path.open()]
    cells, current, probes = [], None, []
    last_probe = None
    for item in frames:
        kind = item["d"]
        if kind == "p":
            last_probe = item
            if current is not None and current.get("_done"):
                current["rss_kb"] = item["rss_kb"]
                current["hwm_kb"] = item["hwm_kb"]
                current["cpu_ms"] = round((item["cpu_ns"] - current["_cpu0"]) / 1e6, 3)
                cells.append(current)
                current = None
            continue
        frame = item.get("f")
        if not frame:
            continue
        name = frame.get("frame")
        t = item["t"] / 1e6
        if kind == "h" and name == "start_cell":
            source = frame["source"]
            if run["composition"].startswith("js-"):
                modules, from_names, syntax = None, None, None
            else:
                modules, from_names, syntax = imports_of(source)
            current = {
                **{k: run[k] for k in ("arm", "prompt", "model", "effort", "composition", "worker")},
                "cell": frame["cell"],
                "source": source,
                "bootstrap": bool((frame.get("prelude") or {}).get("bootstrap")),
                "window": frame.get("window"),
                "imports": modules,
                "from_imports": from_names,
                "cpython_syntax": syntax,
                "host_calls": [],
                "batches": [],
                "output_bytes": 0,
                "yields": 0,
                "cancelled": False,
                "_t0": t,
                "_cpu0": last_probe["cpu_ns"] if last_probe else 0,
                "_calls": {},
                "_wait": None,
                "_waited": 0.0,
                "_intervals": [],
            }
        elif current is None:
            continue
        elif name == "nested_call":
            call = {"name": frame["name"], "t0": t, "ms": None, "ok": None}
            current["_calls"][frame["call"]] = call
            current["host_calls"].append(call)
        elif name == "waiting":
            current["batches"].append(len(frame["calls"]))
            current["_wait"] = (t, set(frame["calls"]))
        elif name == "nested_result":
            call = current["_calls"].get(frame["call"])
            if call:
                current["_intervals"].append((call["t0"], t))
                call["ms"] = round(t - call["t0"], 3)
                call["ok"] = "Ok" in frame["outcome"]
            wait = current["_wait"]
            if wait:
                wait[1].discard(frame["call"])
                if not wait[1]:
                    current["_waited"] += t - wait[0]
                    current["_wait"] = None
        elif name == "output":
            current["output_bytes"] += len(frame["item"].get("text", ""))
        elif name == "yielded":
            current["yields"] += 1
        elif name == "cancel":
            current["cancelled"] = True
        elif name in ("cell_done", "host_lost"):
            outcome = frame.get("outcome", {})
            current["outcome"] = outcome.get("outcome", "lost") if name == "cell_done" else "lost"
            message = outcome.get("message") or frame.get("message") or ""
            if current["outcome"] == "stopped":
                message = str(outcome.get("reason"))
            current["error"] = message if current["outcome"] != "completed" else None
            if run["composition"].startswith("js-"):
                # QuickJS puts the error first and its stack after it.
                current["cause"] = message.strip().splitlines()[0][:200] if message.strip() else ""
            else:
                current["cause"] = first_cause(message) if message else ""
            current["value"] = outcome.get("value")
            current["wall_ms"] = round(t - current["_t0"], 3)
            current["host_wait_ms"] = round(current["_waited"], 3)
            current["engine_ms"] = round(current["wall_ms"] - current["_waited"], 3)
            current["engine_ms_calls"] = round(
                current["wall_ms"] - union_ms(current["_intervals"]), 3)
            current["max_outstanding"] = max_overlap(current["_intervals"])
            current["_done"] = True
            if name == "host_lost":
                cells.append(current)
                current = None
    if current is not None and "outcome" not in current:
        current.update(outcome="unfinished", error=None, cause="", wall_ms=None,
                       host_wait_ms=None, engine_ms=None, engine_ms_calls=None,
                       max_outstanding=max_overlap(current["_intervals"]))
        cells.append(current)
    resources = [f["f"]["usage"] for f in frames if f.get("f", {}).get("frame") == "resource_report"]
    for cell in cells:
        for key in [k for k in cell if k.startswith("_")]:
            del cell[key]
        for call in cell["host_calls"]:
            call.pop("t0", None)
        cell["class"] = classify(cell)
    return cells, (resources[-1] if resources else None)


def median(values):
    values = [v for v in values if v is not None]
    return round(statistics.median(values), 2) if values else None


def p90(values):
    values = sorted(v for v in values if v is not None)
    return round(values[int(0.9 * (len(values) - 1))], 2) if values else None


def main():
    raw, out = Path(sys.argv[1]), Path(sys.argv[2])
    replay = {}
    if "--replay" in sys.argv:
        for line in Path(sys.argv[sys.argv.index("--replay") + 1]).open():
            r = json.loads(line)
            replay[(r["arm"], r["prompt"], r["cell"])] = r
    out.mkdir(parents=True, exist_ok=True)
    all_cells, sessions = [], []
    for run_file in sorted(raw.glob("*/*/run.json")):
        run = json.loads(run_file.read_text())
        stdout = (run_file.parent / "stdout.txt").read_text(errors="replace")
        answer = "\n".join(l for l in stdout.splitlines() if not l.startswith("session_id="))
        cells, usage = [], None
        for trace in sorted(run_file.parent.glob("trace-*.jsonl")):
            found, report = parse_trace(trace, run)
            cells += found
            usage = report or usage
        for cell in cells:
            key = (cell["arm"], cell["prompt"], cell["cell"])
            if key in replay:
                cell["replay"] = replay[key]["verdict"]
        all_cells += cells
        usage_events = []
        events = run_file.parent / "events.jsonl"
        if events.exists():
            usage_events = [e for e in map(json.loads, events.open()) if e["kind"] == "turn.usage"]
        tokens = {k: sum(e.get(k, 0) for e in usage_events) for k in
                  ("model_operations", "input_tokens", "cached_input_tokens", "output_tokens",
                   "reasoning_tokens")} if usage_events else {}
        sessions.append({
            **tokens,
            **run,
            "answer_chars": len(answer.strip()),
            "cells": len(cells),
            "failed_cells": sum(c["outcome"] != "completed" for c in cells),
            "host_calls": sum(len(c["host_calls"]) for c in cells),
            "worker_peak_rss_kb": max([c.get("hwm_kb") or 0 for c in cells] or [0]),
            "worker_report": usage,
        })
    with (out / "cells.jsonl").open("w") as f:
        for cell in all_cells:
            f.write(json.dumps(cell, sort_keys=True) + "\n")
    with (out / "sessions.jsonl").open("w") as f:
        for s in sessions:
            f.write(json.dumps(s, sort_keys=True) + "\n")
    machine = "host1 (Ryzen, 16 threads)"
    if "--machine" in sys.argv:
        machine = sys.argv[sys.argv.index("--machine") + 1]
    (out / "summary.md").write_text(summary(sessions, all_cells, machine))


def summary(sessions, cells, machine):
    lines = ["# Real-transcript census: summary", "",
             f"Generated by `census.py`; every number is [M] on {machine}.", ""]
    arms = sorted({s["arm"] for s in sessions})
    lines += ["## Sessions by arm", "",
              "| arm | sessions | exit 0 | answer | cells | failed cells | host calls | median session s | median worker peak RSS KiB |",
              "|---|---|---|---|---|---|---|---|---|"]
    for arm in arms:
        ss = [s for s in sessions if s["arm"] == arm]
        lines.append(
            f"| {arm} | {len(ss)} | {sum(s['exit'] == 0 for s in ss)} | "
            f"{sum(s['exit'] == 0 and s['answer_chars'] > 0 for s in ss)} | "
            f"{sum(s['cells'] for s in ss)} | {sum(s['failed_cells'] for s in ss)} | "
            f"{sum(s['host_calls'] for s in ss)} | {median([s['wall_s'] for s in ss])} | "
            f"{median([s['worker_peak_rss_kb'] for s in ss])} |")
    lines += ["", "## Cells by arm: time and memory", "",
              "| arm | cells | median wall ms | median engine ms | p90 engine ms | median host call ms | median RSS at end KiB |",
              "|---|---|---|---|---|---|---|"]
    for arm in arms:
        cs = [c for c in cells if c["arm"] == arm]
        calls = [h["ms"] for c in cs for h in c["host_calls"]]
        lines.append(
            f"| {arm} | {len(cs)} | {median([c['wall_ms'] for c in cs])} | "
            f"{median([c['engine_ms'] for c in cs])} | {p90([c['engine_ms'] for c in cs])} | "
            f"{median(calls)} | {median([c.get('rss_kb') for c in cs])} |")
    lines += ["", "## Failed cells by class", "", "| arm | class | count |", "|---|---|---|"]
    for (arm, cls), n in sorted(Counter((c["arm"], c["class"]) for c in cells if c["class"]).items()):
        lines.append(f"| {arm} | {cls} | {n} |")
    lines += ["", "## First causes (all arms)", "", "| worker | cause | count |", "|---|---|---|"]
    for (worker, cause), n in Counter((c["worker"], c["cause"]) for c in cells if c["class"]).most_common(40):
        lines.append(f"| {worker} | `{cause.replace('|', '/')[:120]}` | {n} |")
    lines += ["", "## Imports", "", "| module | cells |", "|---|---|"]
    for module, n in Counter(m for c in cells for m in (c["imports"] or [])).most_common():
        lines.append(f"| `{module}` | {n} |")
    lines += ["", "## Host calls", "", "| tool | calls |", "|---|---|"]
    for name, n in Counter(h["name"] for c in cells for h in c["host_calls"]).most_common():
        lines.append(f"| `{name}` | {n} |")
    batches = [b for c in cells for b in c["batches"]]
    lines += ["", f"Batches: {len(batches)}; largest {max(batches or [0])}; "
              f"cells with a batch over 1: {sum(any(b > 1 for b in c['batches']) for c in cells)}.", ""]
    verdicts = Counter(c.get("replay") for c in cells if c.get("replay"))
    if verdicts:
        lines += ["## Cross-worker replay", "", "| verdict | cells |", "|---|---|"]
        lines += [f"| {v} | {n} |" for v, n in verdicts.most_common()]
        lines.append("")
    return "\n".join(lines)


if __name__ == "__main__":
    main()

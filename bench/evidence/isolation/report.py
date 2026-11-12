#!/usr/bin/env python3
"""Render OUT_DIR/perf.jsonl and OUT_DIR/faults.jsonl as Markdown tables."""

import json
import math
import os
import sys

out = sys.argv[1]


def load(name):
    path = os.path.join(out, name)
    if not os.path.exists(path):
        return []
    return [json.loads(line) for line in open(path) if line.strip()]


perf = load("perf.jsonl")
# macOS has no PSS; the harness reads phys_footprint there (backend.rs).
build = os.path.join(out, "build.txt")
PSS = "phys_footprint" if os.path.exists(build) and "Darwin" in open(build).read() else "PSS"
faults = load("faults.jsonl")


def sec(name):
    return [r for r in perf if r.get("section") == name]


def f(x, digits=1):
    if x is None:
        return "—"
    if isinstance(x, dict):
        x = x.get("median")
    if abs(x) >= 100:
        return f"{x:,.0f}"
    return f"{x:,.{digits}f}"


def table(head, rows):
    print("| " + " | ".join(head) + " |")
    print("|" + "|".join("---" for _ in head) + "|")
    for r in rows:
        print("| " + " | ".join(str(c) for c in r) + " |")
    print()


size = os.path.join(out, "size.txt")
if os.path.exists(size):
    print("## mm-wasm guest size (shipped build) [M]\n")
    for line in open(size):
        print("- " + line.strip())
    print()

if perf:
    print("## Startup [M]\n")
    rows = []
    for r in sec("cold"):
        rows.append([r["mode"], f(r["first_cell_us_in_process"]["median"] / 1000, 2),
                     f(r["module_load_us"]["median"] / 1000, 2),
                     f(r["process_spawn_to_exit_us"]["median"] / 1000, 2)])
    table(["cold: mode", "main → first cell done (ms)", "module load (ms)", "spawn → exit (ms)"], rows)
    rows = []
    for r in sec("warm"):
        rows.append([r["mode"], r["what"], f(r.get("session_ready_us")), f(r["first_cell_done_us"]),
                     f(r["first_cell_done_us"]["p99"])])
    table(["warm: mode", "what", "session ready (µs)", "first cell done (µs, median)", "p99 (µs)"], rows)

    print("## Host calls [M]\n")
    rows = []
    for r in sec("hostcall"):
        rows.append([r["mode"], f(r["host_call_round_trip_us"], 2), f(r["sync_loop_us_per_iteration"], 2),
                     f(r["python_call_loop_us_per_iteration"], 2), f(r["codemode_cell_us"]["median"] / 1000, 2),
                     f(r["codemode_us_per_host_call"], 2)])
    table(["mode", "host call round trip (µs)", "loop with host call (µs/iter)",
           "loop with Python call (µs/iter)", "codemode cell, 400 calls (ms)", "codemode µs per call"], rows)

    print("## Bench programs, ms per run (median) [M]\n")
    b = sec("bench")
    modes = list(dict.fromkeys(r["mode"] for r in b))
    progs = list(dict.fromkeys(r["program"] for r in b))
    ms = {(r["mode"], r["program"]): r["ms"]["median"] for r in b}
    rows = []
    for p in progs:
        rows.append([p] + [f(ms.get((m, p)), 2) for m in modes])
    geo = []
    for m in modes:
        ratios = [ms[(m, p)] / ms[("native", p)] for p in progs
                  if (m, p) in ms and ("native", p) in ms and p != "startup"]
        geo.append(f"{math.exp(sum(map(math.log, ratios)) / len(ratios)):.2f}×" if ratios else "—")
    rows.append(["**geomean vs native** (excl. startup)"] + geo)
    table(["program"] + modes, rows)

    print("## Snapshots [M]\n")
    rows = []
    for r in sec("snapshot"):
        rows.append([r["state"], r["mode"], f"{r['bytes']:,}", f(r["snapshot_us"]), f(r["restore_us"])])
    table(["state", "mode", "bytes", "snapshot (µs)", "restore (µs)"], rows)
    rows = [[r["from"], r["to"], r["result"]] for r in sec("cross")]
    if rows:
        table(["saved in", "restored in", "result"], rows)

    print("## Memory per session (100 sessions) [M]\n")
    rows = []
    for r in sec("memory"):
        rows.append([r["mode"], r["state"], f(r["host_rss_delta_kib_per_session"]),
                     f(r.get("linear_memory_kib_per_session")),
                     f(r.get("worker_rss_kib")), f(r.get("worker_pss_kib"))])
    table(["mode", "state", "host RSS Δ (KiB/session)", "wasm linear memory (KiB)",
           "worker RSS (KiB)", f"worker {PSS} (KiB)"], rows)

    print("## Many sessions on one host (codemode cells) [M]\n")
    rows = []
    for r in sec("scale"):
        lat = r.get("cell_latency_us") or {}
        rows.append([r["mode"], r["sessions"], r["threads"], r["cells"], r["errors"], f(r["cells_per_s"]),
                     f(lat.get("median", 0) / 1000, 2), f(lat.get("p99", 0) / 1000, 2),
                     f(r["host_rss_kib"] / 1024), f(r["workers_pss_kib_sum"] / 1024)])
    table(["mode", "sessions", "threads", "cells", "errors", "cells/s", "p50 (ms)", "p99 (ms)",
           "host RSS (MiB)", f"workers {PSS} (MiB)"], rows)


def short(v):
    if v is None:
        return "—"
    if isinstance(v, dict):
        if "died" in v:
            return "lost: " + v["died"]
        if v.get("ok") == "complete":
            return "completed: " + v.get("output", "").strip().replace("\n", " ")[:40]
        if v.get("ok") == "error":
            return v.get("type", "") + (": " + v["message"][:50] if v.get("message") else "")
        return str(v.get("ok"))[:60]
    return str(v)


if faults:
    print("## Fault matrix [M]\n")
    rows = []
    for r in faults:
        latency = r.get("latency_ms")
        returned = r.get("control_returned_ms")
        timing = ""
        if latency is not None:
            timing = f"{latency:.3f} ms after the stop"
        elif returned is not None:
            timing = f"{returned:.0f} ms after start"
        host = r.get("host", "")
        if host.startswith("died"):
            rows.append([r["label"], f"**{host}**", "; ".join(r.get("stderr", []))[:90], "lost with the host",
                         "lost with the host", "—", "—"])
        else:
            victim = short(r.get("victim"))
            if r.get("host_hwm_kib"):
                victim += f" (host peak RSS {r['host_hwm_kib'] // 1024} MiB)"
            rows.append([r["label"], host, victim, short(r.get("victim_after")),
                         r.get("bystander", "—"), r.get("fresh", "—"), timing])
    table(["scenario", "host", "victim", "victim session after", "bystander session", "fresh session",
           "time to control"], rows)

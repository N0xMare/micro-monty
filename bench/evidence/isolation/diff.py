#!/usr/bin/env python3
"""Rows of two fault matrices that differ: diff.py OLD/faults.jsonl NEW/faults.jsonl.

A row differs when the host, the victim, the victim's session afterwards or
the bystander changed, when the time to control moved by more than half
(and 50 ms), or when the host's peak RSS moved by more than 20 MiB.
"""
import json
import sys


def short(x):
    if not isinstance(x, dict):
        return x
    if "died" in x:
        return "lost: " + x["died"][:50]
    if x.get("ok") == "complete":
        return "completed: " + x.get("output", "").strip()[:30]
    if x.get("ok") == "error":
        return x.get("type")
    return str(x.get("ok"))[:40]


def load(path):
    rows = {}
    for line in open(path):
        r = json.loads(line)
        rows[r["label"]] = {
            "host": r.get("host", "")[:30],
            "victim": short(r.get("victim")),
            "after": short(r.get("victim_after")),
            "bystander": r.get("bystander"),
            "t": r.get("latency_ms", r.get("control_returned_ms")),
            "hwm": r.get("host_hwm_kib"),
        }
    return rows


old, new = load(sys.argv[1]), load(sys.argv[2])
print("| row | what changed |\n|---|---|")
for label, a in old.items():
    b = new.get(label)
    if b is None:
        print(f"| {label} | missing in the new run |")
        continue
    changes = [f"{k}: {a[k]} → {b[k]}" for k in ("host", "victim", "after", "bystander") if a[k] != b[k]]
    if a["t"] and b["t"] and abs(a["t"] - b["t"]) > max(50, 0.5 * a["t"]):
        changes.append(f"time to control: {a['t']:.1f} → {b['t']:.1f} ms")
    if a["hwm"] and b["hwm"] and abs(a["hwm"] - b["hwm"]) > 20 << 10:
        changes.append(f"host peak RSS: {a['hwm'] >> 10} → {b['hwm'] >> 10} MiB")
    if changes:
        print(f"| {label} | {'; '.join(changes)} |")

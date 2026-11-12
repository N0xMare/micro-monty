#!/usr/bin/env python3
"""Run the fault matrix: one `iso-harness fault` process per row.

Usage: faults.py OUT_DIR   (run.sh sets ISO_* paths to the built variants)

Each row's process holds a bystander session stopped at a host call, then
a victim session meets the fault. The process prints a JSON verdict, or
dies; a death is recorded with its signal and the stderr tail. Writes
OUT_DIR/faults.jsonl.
"""

import json
import os
import signal
import subprocess
import sys
import time

E = os.environ
H = E["ISO_HARNESS"]  # release (panic=abort), with wasmtime
HU = E["ISO_HARNESS_UNWIND"]  # release-unwind
HS = E["ISO_HARNESS_SHIPPED"]  # immediate-abort, native only
W, WU, WS = E["ISO_WORKER"], E["ISO_WORKER_UNWIND"], E["ISO_WORKER_SHIPPED"]
G, GS = E["ISO_GUEST"], E["ISO_GUEST_SHIPPED"]
# The mm-wasm v1 guests (release with test-hooks; shipped uu with test-hooks).
AUS = E.get("ISO_ABI_UU_SHIPPED", "")

rows = []


def row(label, args, harness=H, worker=W, guest=G, timeout=120, env=None):
    rows.append((label, args, harness, worker, guest, timeout, env or {}))


# Rust stack exhaustion: native thread stack, wasm max_wasm_stack, worker stack.
for case in ["stack-cyclic-dict", "stack-frozenset-600"]:
    for kib in [256, 512, 1024, 2048, 8192]:
        row(f"{case} native thread {kib} KiB", ["native", case, f"host_stack_kib={kib}"])
    for kib in [512, 1024, 4096]:
        row(f"{case} wasm max_wasm_stack {kib} KiB", ["wasm", case, f"max_wasm_stack_kib={kib}"])
    for kib in [256, 512, 16384]:
        row(f"{case} child worker stack {kib} KiB", ["child", case, f"stack_kib={kib}"])

# The mm-wasm v1 interface through the reference host (abi: wasip1 guest,
# abi-uu: wasm32-unknown-unknown guest): async stop is a park, no trap.
for m in ["abi", "abi-uu"]:
    for case in ["stack-cyclic-dict", "stack-frozenset-600"]:
        for kib in [512, 1024, 4096]:
            row(f"{case} {m} max_wasm_stack {kib} KiB", [m, case, f"max_wasm_stack_kib={kib}"])
    row(f"panic {m} release guest", [m, "panic"])
    for src in ["loop", "native"]:
        row(f"runaway {src} {m} engine fuel 50M", [m, "runaway", f"src={src}", "guard=fuel"])
        row(f"runaway {src} {m} fuel slices 10k + host flag", [m, "runaway", f"src={src}", "guard=slice"])
        row(f"runaway {src} {m} park at 200 ms (grace 100 ms)", [m, "runaway", f"src={src}", "guard=park"])
        row(f"runaway {src} {m} call budget 200 ms", [m, "runaway", f"src={src}", "guard=wasm", "call_budget_ms=200"])
    row(f"memory arena 64 MiB {m}", [m, "memory-arena", "memory_mib=64"])
if AUS:
    row("panic abi-uu shipped guest", ["abi-uu", "panic"], env={"ISO_ABI_UU": AUS})

# Engine panic (injected at `start`), by build.
row("panic native release (panic=abort)", ["native", "panic"])
row("panic native release-unwind", ["native", "panic"], harness=HU)
row("panic native shipped (immediate-abort)", ["native", "panic"], harness=HS)
row("panic wasm release guest", ["wasm", "panic"])
row("panic wasm shipped guest", ["wasm", "panic"], guest=GS)
row("panic child release worker (panic=abort)", ["child", "panic"])
row("panic child release-unwind worker", ["child", "panic"], worker=WU)
row("panic child shipped worker", ["child", "panic"], worker=WS)

# Runaway: a Python loop, and one long native call (bigint multiply).
for src in ["loop", "native"]:
    for mode in ["native", "wasm", "child"]:
        row(f"runaway {src} {mode} engine fuel 50M", [mode, "runaway", f"src={src}", "guard=fuel"])
        row(f"runaway {src} {mode} fuel slices 10k + host flag", [mode, "runaway", f"src={src}", "guard=slice"])
        row(f"runaway {src} {mode} async stop at 200 ms", [mode, "runaway", f"src={src}", "guard=park"])
    row(f"runaway {src} wasm epoch budget 200 ms", ["wasm", "runaway", f"src={src}", "guard=wasm", "epoch_ticks=20"])
    row(f"runaway {src} wasm-fuel 2e9", ["wasm-fuel", "runaway", f"src={src}", "guard=wasm", "wasm_fuel=2000000000"])
    row(f"runaway {src} child SIGKILL at 200 ms", ["child", "runaway", f"src={src}", "guard=kill"])
    row(f"runaway {src} child RLIMIT_CPU 1 s", ["child", "runaway", f"src={src}", "guard=none", "prlimit=--cpu=1"])

# Memory: the arena limit, and allocations outside it.
for mode in ["native", "wasm", "child"]:
    row(f"memory arena 64 MiB {mode}", [mode, "memory-arena", "memory_mib=64"])

# Allocations outside arena accounting: the completion value and a host-call
# argument are converted to `HostValue` trees on the Rust heap (56 MB of
# arena list becomes hundreds of MB of host memory).
OUTSIDE = [
    ("completion value", "[0] * 7_000_000"),
    ("host-call argument", "x = tool([0] * 7_000_000)"),
]
for what, src in OUTSIDE:
    for mode, extra in [
        ("native", []),
        ("wasm", ["memory_limit_mib=256"]),
        ("abi", ["memory_limit_mib=256"]),
        ("abi-uu", ["memory_limit_mib=256"]),
        ("child", []),
        ("child", ["prlimit=--as=268435456"]),
    ]:
        row(f"memory outside arena ({what}) {mode} {' '.join(extra)}".strip(),
            [mode, "memory-source", "memory_mib=64", f"src={src}", *extra])


def run(label, args, harness, worker, guest, timeout, extra):
    env = dict(E, ISO_WORKER=worker, ISO_GUEST=guest, **extra)
    start = time.monotonic()
    try:
        p = subprocess.run([harness, "fault", *args], env=env, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return {"label": label, "args": args, "host": f"hung > {timeout} s (killed)"}
    record = {"label": label, "args": args, "wall_s": round(time.monotonic() - start, 3)}
    if p.returncode == 0:
        record.update(json.loads(p.stdout.decode().strip().splitlines()[-1]))
    else:
        rc = p.returncode
        how = f"killed by {signal.Signals(-rc).name}" if rc < 0 else f"exit {rc}"
        tail = p.stderr.decode(errors="replace").strip().splitlines()[-2:]
        record.update({"host": f"died: {how}", "stderr": tail})
    return record


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    only = sys.argv[2] if len(sys.argv) > 2 else ""
    with open(os.path.join(out, "faults.jsonl"), "w") as f:
        for r in rows:
            if only and only not in r[0]:
                continue
            record = run(*r)
            print(json.dumps(record), flush=True)
            f.write(json.dumps(record) + "\n")


main()

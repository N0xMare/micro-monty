#!/usr/bin/env python3
"""Runs real `plak run` sessions on a micro-monty worktree, recording every cell.

Each arm is (account, model, effort, composition, worker). Every prompt in
`prompts.json` runs once per arm, in a worktree reset to a clean checkout
first. The Python worker is wrapped by `cell-tracer` (tracer/), which logs
every cell-protocol frame, so the same records come from plak's Monty worker
and from `mm-cell-worker` without changing plak. The `jsptc` composition
(plak's js-ptc) wraps plak's QuickJS worker, `plakous-js-host`, the same way
through `PLAK_JS_HOST`; its only worker is `qjs`.

    python3 run_sessions.py --raw DIR --plak PLAK --monty WORKER --mm WORKER \
        --tracer TRACER --worktrees W1,W2,... [--arms codex-ptc-monty,...]

Raw output (traces, stdout, stderr, a `run.json` per session) goes to DIR;
`census.py` turns it into the committed records. Arms run in parallel, one
worktree each; prompts within an arm run in order.
"""

import argparse
import json
import os
import queue
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent

MODELS = {
    "codex": ("codex", "gpt-6-luna", "high"),
    "grok": ("grok", "grok-4.7", "medium"),
}
COMPOSITIONS = {"ptc": "py-ptc", "sdk": "py-sdk", "jsptc": "js-ptc"}
MM_IDENTITY = "micro-monty/micro-monty-cell-adapter.v1"


def all_arms():
    return [
        f"{m}-{c}-{w}"
        for m in MODELS
        for c in ("ptc", "sdk")
        for w in ("monty", "mm", "native", "wasm", "cache")
    ] + [f"{m}-jsptc-qjs" for m in MODELS]


def wrapper(directory: Path, tracer: str, worker: str) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    script = directory / "worker.sh"
    script.write_text(f"#!/bin/sh\nexec {tracer} {directory} {worker}\n")
    script.chmod(0o755)
    return script.resolve()


def reset(worktree: Path):
    subprocess.run(["git", "checkout", "-q", "--", "."], cwd=worktree, check=True)
    subprocess.run(["git", "clean", "-q", "-fd"], cwd=worktree, check=True)


def run_one(args, arm: str, prompt_id: str, prompt: str, worktree: Path):
    model, composition, worker = arm.split("-")
    account, model_id, effort = MODELS[model]
    out = Path(args.raw) / arm / prompt_id
    if (out / "run.json").exists() and not args.force:
        return
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("trace-*.jsonl"):
        old.unlink()
    env = dict(os.environ)
    env.update(HOME=args.home, XDG_STATE_HOME=args.state, PLAK_VERBOSE="1")
    for name in ("PLAK_PY_HOST_IDENTITY", "PLAK_PY_INPROC", "PLAK_PY_INPROC_TRACE", "PLAK_JS_HOST"):
        env.pop(name, None)
    plak = args.plak
    if worker == "qjs":
        env["PLAK_JS_HOST"] = str(wrapper(out, args.tracer, args.qjs))
    elif worker in ("monty", "mm"):
        real = args.monty if worker == "monty" else args.mm
        env["PLAK_PY_HOST"] = str(wrapper(out, args.tracer, real))
    else:
        # In process (plak branch mm-inproc): the worker is a thread of plak;
        # PLAK_PY_HOST is only the attached artifact plak hashes.
        plak = args.plak_cache if worker == "cache" else args.plak_inproc
        env.update(
            PLAK_PY_HOST=args.mm,
            PLAK_PY_INPROC="wasm" if worker == "wasm" else "native",
            PLAK_PY_INPROC_WASM=args.wasm,
            PLAK_PY_INPROC_TRACE=str(out.resolve()),
        )
    if worker not in ("monty", "qjs"):
        env["PLAK_PY_HOST_IDENTITY"] = MM_IDENTITY
    for name, value in args.extra_env:
        env[name] = value
    reset(worktree)
    command = [
        plak, "run", "--print-session-id",
        "--account", account, "--model", model_id, "--effort", effort,
        "--composition", COMPOSITIONS[composition],
        "--timeout", str(args.timeout),
        "--", prompt,
    ]
    started = time.time()
    with open(out / "stdout.txt", "wb") as stdout, open(out / "stderr.txt", "wb") as stderr:
        status = subprocess.run(command, cwd=worktree, env=env, stdout=stdout, stderr=stderr).returncode
    wall = time.time() - started
    # plak's event log of the session: per-turn usage (model operations, tokens).
    session_id = next((l.split("=", 1)[1].strip() for l in (out / "stdout.txt").read_text(
        errors="replace").splitlines() if l.startswith("session_id=")), None)
    if session_id:
        for events in Path(args.home).glob(f".plak/state/*/sessions/{session_id}.events.jsonl"):
            shutil.copyfile(events, out / "events.jsonl")
    dirty = subprocess.run(
        ["git", "status", "--porcelain"], cwd=worktree, capture_output=True, text=True
    ).stdout
    record = {
        "arm": arm, "prompt": prompt_id, "account": account, "model": model_id,
        "effort": effort, "composition": COMPOSITIONS[composition], "worker": worker,
        "exit": status, "wall_s": round(wall, 3), "started": started,
        "worktree_changes": dirty.splitlines(),
    }
    (out / "run.json").write_text(json.dumps(record, indent=1) + "\n")
    print(f"{arm} {prompt_id}: exit {status} in {wall:.0f}s", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--raw", required=True)
    parser.add_argument("--plak", required=True)
    parser.add_argument("--plak-inproc", help="plak built from branch mm-inproc")
    parser.add_argument("--plak-cache", help="the same, linked to the decoded-stdlib cache patch")
    parser.add_argument("--wasm", help="the shipped mm-wasm v1 guest (mm_wasm_guest.wasm)")
    parser.add_argument("--home", required=True, help="HOME for the isolated plak")
    parser.add_argument("--state", default=str(Path.home() / ".local/state"),
                        help="XDG_STATE_HOME: the shared credential store")
    parser.add_argument("--monty", help="plak's Monty worker, for the monty arms")
    parser.add_argument("--mm", help="mm-cell-worker, for the mm and in-process arms")
    parser.add_argument("--qjs", help="plak's QuickJS worker (plakous-js-host), for the qjs arms")
    parser.add_argument("--tracer", required=True)
    parser.add_argument("--worktrees", required=True)
    parser.add_argument("--arms", default=",".join(all_arms()))
    parser.add_argument("--prompts", default=str(HERE / "prompts.json"))
    parser.add_argument("--only", default="", help="comma-separated prompt ids")
    parser.add_argument("--timeout", type=int, default=1200)
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--env", action="append", default=[],
                        help="NAME=VALUE added to plak's environment")
    args = parser.parse_args()
    args.extra_env = [tuple(item.split("=", 1)) for item in args.env]
    prompts = json.loads(Path(args.prompts).read_text())
    if args.only:
        keep = set(args.only.split(","))
        prompts = [p for p in prompts if p["id"] in keep]
    arms = args.arms.split(",")
    worktrees = [Path(w) for w in args.worktrees.split(",")]
    jobs = queue.Queue()
    for arm in arms:
        jobs.put(arm)

    def lane(worktree: Path):
        while True:
            try:
                arm = jobs.get_nowait()
            except queue.Empty:
                return
            for prompt in prompts:
                try:
                    run_one(args, arm, prompt["id"], prompt["prompt"], worktree)
                except Exception as error:  # keep the other runs going
                    print(f"{arm} {prompt['id']}: harness error {error}", file=sys.stderr)

    threads = [threading.Thread(target=lane, args=(w,)) for w in worktrees]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()


if __name__ == "__main__":
    main()

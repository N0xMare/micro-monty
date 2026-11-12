"""Bounded public-wheel API probes, with a genuinely new-process restoration.

Only restore bytes made by this probe; Monty dumps are trusted-input APIs.
No timings, random inputs, source patches, or arbitrary malformed snapshots.
"""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from pydantic_monty import (
    CollectString, FunctionSnapshot, FutureSnapshot, Monty, MontyComplete,
    MontyRuntimeError, NameLookupSnapshot,
)


def rejected(label, exception_type, operation, contains):
    try:
        operation()
    except exception_type as error:
        message = str(error)
        assert contains in message, (label, message)
        return {"probe": label, "exception": type(error).__name__, "message": message}
    raise AssertionError(f"{label}: expected {exception_type.__name__}")


def child_restore(path):
    with Monty(min_processes=1, max_processes=1, request_timeout=3) as pool:
        with pool.checkout() as session:
            output = CollectString()
            snap = session.load_snapshot(Path(path).read_bytes(), print_callback=output)
            assert isinstance(snap, FunctionSnapshot) and snap.args == (40,)
            done = snap.resume({"return_value": 41})
            assert isinstance(done, MontyComplete) and done.output == 42
            assert output.output == "after\n", output.output
            return {"probe": "new_process_restore", "result": done.output,
                    "post_restore_print": output.output, "position": snap.position.dict()}


def run():
    rows = []
    # A single worker, no networking, no filesystem mounts, bounded per-turn watchdog.
    with Monty(min_processes=1, max_processes=1, request_timeout=3) as pool:
        with pool.checkout() as session:
            assert session.feed_run("x = 40\nx + 2") == 42
            idle = session.dump()
            rows.append({"probe": "idle_dump", "bytes": len(idle)})
        with pool.checkout() as session:
            session.load_session(idle)
            assert session.feed_run("x + 2") == 42
            rows.append(rejected("load_after_feed", RuntimeError,
                                 lambda: session.load_session(idle), "fresh session"))
        with pool.checkout() as session:
            rows.append(rejected("idle_wrong_loader", RuntimeError,
                                 lambda: session.load_snapshot(idle), "idle"))
        with pool.checkout() as session:
            source = "x = 40\nprint('before')\ny = host(x)\nprint('after')\ny + 1"
            output = CollectString()
            snap = session.feed_start(source, print_callback=output)
            assert isinstance(snap, FunctionSnapshot) and snap.function_name == "host"
            assert snap.args == (40,) and not snap.is_os_function
            assert source.encode()[snap.position.start:snap.position.end] == b"host(x)"
            assert output.output == "before\n"
            paused = snap.dump()
            position = snap.position.dict()
            rows.append(rejected("feed_while_suspended", RuntimeError,
                                 lambda: session.feed_run("1"), "suspension is awaiting an answer"))
            done = snap.resume({"return_value": 41})
            assert isinstance(done, MontyComplete) and done.output == 42
            rows.append(rejected("double_resume", RuntimeError,
                                 lambda: snap.resume({"return_value": 99}), "already been resumed"))
            assert output.output == "before\nafter\n"
            rows.append({"probe": "function_dump_resume", "bytes": len(paused),
                         "result": done.output, "position": position})
        with pool.checkout() as session:
            rows.append(rejected("suspended_wrong_loader", RuntimeError,
                                 lambda: session.load_session(paused), "suspended"))
        # Restore the same trusted image twice: branches own independent workers' states.
        for answer in (41, 99):
            with pool.checkout() as session:
                output = CollectString()
                snap = session.load_snapshot(paused, print_callback=output)
                assert snap.position.dict() == position
                done = snap.resume({"return_value": answer})
                assert done.output == answer + 1 and output.output == "after\n"
                rows.append({"probe": "restore_branch", "answer": answer, "result": done.output})
        with pool.checkout() as session:
            snap = session.feed_start("unknown + 2")
            assert isinstance(snap, NameLookupSnapshot) and snap.variable_name == "unknown"
            name_dump = snap.dump()
        with pool.checkout() as session:
            snap = session.load_snapshot(name_dump)
            assert isinstance(snap, NameLookupSnapshot)
            assert snap.resume(value=40).output == 42
            rows.append({"probe": "name_lookup_dump_resume", "result": 42})
        with pool.checkout() as session:
            snap = session.feed_start("open('/not-mounted.txt').read()")
            assert isinstance(snap, FunctionSnapshot) and snap.is_os_function
            os_dump = snap.dump()
            os_name = str(snap.function_name)
        with pool.checkout() as session:
            snap = session.load_snapshot(os_dump)
            assert isinstance(snap, FunctionSnapshot) and snap.is_os_function
            rows.append(rejected("os_dump_resume_exception", MontyRuntimeError,
                                 lambda: snap.resume({"exc_type": "FileNotFoundError", "message": "probe"}),
                                 "FileNotFoundError"))
            rows.append({"probe": "os_snapshot", "function": os_name})
        with pool.checkout() as session:
            snap = session.feed_start("await host()")
            assert isinstance(snap, FunctionSnapshot)
            future = snap.resume({"future": ...})
            assert isinstance(future, FutureSnapshot)
            future_dump = future.dump()
        with pool.checkout() as session:
            future = session.load_snapshot(future_dump)
            assert isinstance(future, FutureSnapshot)
            rows.append(rejected("restored_future_auto", RuntimeError,
                                 future.resume_auto, "async"))
            [call_id] = future.pending_call_ids
            done = future.resume({call_id: {"return_value": 42}})
            assert isinstance(done, MontyComplete) and done.output == 42
            rows.append({"probe": "future_dump_manual_resume", "result": done.output})
        with pool.checkout() as session:
            done = session.feed_start("sum(range(100))")
            assert isinstance(done, MontyComplete) and done.output == 4950
            rows.append({"probe": "pure_compute_no_intermediate_suspension", "result": done.output})
            rows.append(rejected("no_fuel_keyword", TypeError,
                                 lambda: session.feed_start("1", fuel=1), "fuel"))
        # Deadline abort is not a resumable fuel/time-slice snapshot; discard this session.
        with pool.checkout(limits={"max_turn_duration_secs": 0.01}) as session:
            rows.append(rejected("pure_compute_deadline", MontyRuntimeError,
                                 lambda: session.feed_start("while True: pass"), "TimeoutError"))
    with tempfile.TemporaryDirectory(prefix="monty-restore-", dir="/tmp/mm-evidence-baselines") as directory:
        path = Path(directory) / "trusted.dump"
        path.write_bytes(paused)
        child = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), "--restore-child", str(path)],
            capture_output=True, text=True, timeout=10, check=True,
        )
        rows.append(json.loads(child.stdout))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--restore-child", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = child_restore(args.restore_child) if args.restore_child else run()
    rendered = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered)
    print(rendered, end="")


if __name__ == "__main__":
    main()

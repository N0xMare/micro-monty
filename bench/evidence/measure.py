"""Bounded native evidence campaign, not a general benchmark framework.

Run alone: taskset -c 2 python3 bench/evidence/measure.py SECTION NEW_OUTPUT
Sections: warm, lifecycle, persistence (needs the delta candidate's
persistence.rs, local branch evidence-campaign), programs. Builds are found under
$MM_EVIDENCE_PREFIX (default /tmp/mm-evidence); see MICRO/BASE below.
Outcomes: pass, wrong_result, error (nonzero exit), crash (signal), timeout,
harness_error (unreadable output), fresh_restore_failure. summary.json scores
only cells whose every observation passed. Tests: test_measure.py.
--reps defaults to 21 independent processes per cell; each process reports its own warm batch. A round -1
outer warmup is retained but excluded. Output is append+flush after every cell.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time
import zlib

ROOT = Path(__file__).resolve().parents[2]
# Every build lives under one prefix; `test_measure.py` points it at fixtures.
PREFIX = os.environ.get("MM_EVIDENCE_PREFIX", "/tmp/mm-evidence")
MICRO = Path(f"{PREFIX}-layers")  # {main,candidate}/probe from build_micro.sh, persistence
BASE = Path(f"{PREFIX}-baselines")  # bin/: monty-warm, numeric-{qjs,dquickts,qpy}
QPY = f"{PREFIX}-dquickts/py"
DJS = Path(f"{PREFIX}-djs/probe-target/release/djs-probe")
PYTHON = os.environ.get("PYTHON", str(Path.home() / ".local/bin/python3.14"))
TIMEOUT = 60
WARM_REPS = 3


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def invoke(command, parse=True, env=None):
    """Distinguishes timeout, crash (signal), error exit and unreadable output.

    Every probe validates its own result and exits nonzero on a mismatch, so a
    wrong answer arrives here as "error"; callers add their own checks on top.
    """
    command = list(map(str, command))
    start = time.perf_counter_ns()
    try:
        process = subprocess.run(command, capture_output=True, text=True, timeout=TIMEOUT,
                                 env=env)
    except subprocess.TimeoutExpired as error:
        return dict(outcome="timeout", error=str(error), command=command)
    result = dict(command=command, returncode=process.returncode,
                  stdout=process.stdout, stderr=process.stderr,
                  process_ns=time.perf_counter_ns() - start)
    if process.returncode < 0:
        result["outcome"] = "crash"
    elif process.returncode:
        result["outcome"] = "error"
    elif not parse:
        result["outcome"] = "pass"
    else:
        try:
            result["data"] = [json.loads(line) for line in process.stdout.splitlines()]
            result["outcome"] = "pass" if result["data"] else "harness_error"
        except ValueError as error:
            result.update(outcome="harness_error", error=str(error))
    return result


def check_warm(result, n):
    """Warm output must report this N's sum and exactly WARM_REPS positive samples."""
    if result["outcome"] != "pass":
        return
    data = result["data"][-1]
    samples = data.get("warm_ns")
    if data.get("checked_result") != n * (n - 1) // 2:
        result["outcome"] = "wrong_result"
    elif not (isinstance(samples, list) and len(samples) == WARM_REPS
              and all(isinstance(s, int) and s > 0 for s in samples)):
        result["outcome"] = "harness_error"


def warm_commands(n):
    reps = str(WARM_REPS)
    return [
        ("main", [str(MICRO / "main/probe"), "warm", str(n), reps]),
        ("candidate", [str(MICRO / "candidate/probe"), "warm", str(n), reps]),
        ("qjs", [str(BASE / "bin/numeric-qjs"), "js", str(n), reps]),
        ("dquickts", [str(BASE / "bin/numeric-dquickts"), "js", str(n), reps]),
        ("qpy", [str(BASE / "bin/numeric-qpy"), "py", str(n), reps, QPY]),
        ("monty", [str(BASE / "bin/monty-warm"), "numeric", str(n), reps]),
        ("djs", [str(DJS), "warm", str(n), reps]),
        ("cpython", [PYTHON, str(ROOT / "bench/evidence/baselines/cpython-numeric.py"), str(n), reps]),
    ]


def cell_key(sample):
    fields = ["section", "engine", "program", "mode", "n", "count", "rows", "changed"]
    return tuple((f, sample[f]) for f in fields if f in sample)


def summarize(samples):
    """Per cell: outcome counts, and a median only when every round passed.

    A cell with any non-pass observation (warmups included) gets no score, so
    failures can never shrink the denominator or improve a median.
    """
    cells = {}
    for sample in samples:
        cells.setdefault(cell_key(sample), []).append(sample)
    summary = []
    for key, group in cells.items():
        outcomes = {}
        for sample in group:
            outcomes[sample["outcome"]] = outcomes.get(sample["outcome"], 0) + 1
        row = dict(key, outcomes=outcomes)
        timed = [s for s in group if s["block"] >= 0]
        if set(outcomes) == {"pass"}:
            row["rounds"] = len(timed)
            row["median_process_ms"] = statistics.median(s["process_ns"] for s in timed) / 1e6
            if group[0]["section"] == "warm":
                row["median_warm_ms"] = statistics.median(
                    statistics.median(s["data"][-1]["warm_ns"]) for s in timed) / 1e6
        summary.append(row)
    return summary


def compression(directory):
    # Reading files is outside codec timers. These are zlib level 1 independent
    # streams, NOT dictionary sharing, fsync, storage acknowledgement or RSS.
    samples = []
    for step in range(9):
        raw = (directory / f"{step}.mmsnap").read_bytes()
        start = time.perf_counter_ns()
        packed = zlib.compress(raw, level=1)
        encode_ns = time.perf_counter_ns() - start
        start = time.perf_counter_ns()
        decoded = zlib.decompress(packed)
        decode_ns = time.perf_counter_ns() - start
        assert decoded == raw, "compression round trip"
        samples.append(dict(raw_bytes=len(raw), compressed_bytes=len(packed),
                            compress_ns=encode_ns, decompress_ns=decode_ns,
                            raw_sha256=hashlib.sha256(raw).hexdigest()))
    return samples


def main():
    global TIMEOUT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("section", choices=["warm", "lifecycle", "persistence", "programs"])
    parser.add_argument("--variants", default="main,candidate",
                        help="programs: comma-separated MICRO/<variant>/mm builds")
    parser.add_argument("out", type=Path)
    parser.add_argument("--reps", type=int, default=21)
    parser.add_argument("--timeout", type=int, default=TIMEOUT, help="seconds per process")
    args = parser.parse_args()
    assert 1 <= args.reps <= 100 and args.timeout >= 1
    TIMEOUT = args.timeout
    # Every engine the section needs is required; none is silently dropped.
    binaries = {"warm": [command[0] for _, command in warm_commands(1)],
                "lifecycle": [str(MICRO / name / "probe") for name in ["main", "candidate"]],
                "persistence": [str(MICRO / "persistence")],
                "programs": [str(MICRO / v / "mm") for v in args.variants.split(",")]}[args.section]
    missing = [b for b in binaries if not Path(b).is_file()]
    if missing:
        sys.exit("missing required binaries:\n  " + "\n  ".join(missing))
    args.out.mkdir(parents=True, exist_ok=False)
    manifest = dict(
        section=args.section, repetitions=args.reps, outer_warmups=1,
        created_unix_ns=time.time_ns(), machine=platform.platform(),
        cpu=Path("/proc/cpuinfo").read_text().split("model name", 1)[1].splitlines()[0],
        affinity=sorted(os.sched_getaffinity(0)), python=sys.version,
        zlib=zlib.ZLIB_RUNTIME_VERSION, binaries={p: digest(p) for p in binaries},
        variant_env={v: (MICRO / v / "env").read_text() for v in args.variants.split(",")
                     if (MICRO / v / "env").is_file()} if args.section == "programs" else {},
        source_hashes={str(p.relative_to(ROOT)): digest(p) for p in (ROOT / "bench/evidence").rglob("*") if p.is_file()},
        sampling="rotating order; independent process per round; no tails inferred",
    )
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2))
    failures = 0
    samples = []
    with (args.out / "samples.jsonl").open("x") as output:
        def record(value):
            nonlocal failures
            failures += value.get("outcome") != "pass"
            samples.append(value)
            output.write(json.dumps(value) + "\n")
            output.flush()

        if args.section == "warm":
            for n in [1000, 100000, 3000000]:
                commands = warm_commands(n)
                for block in range(-1, args.reps):
                    order = commands[block % len(commands):] + commands[:block % len(commands)]
                    for engine, command in order:
                        result = invoke(command)
                        check_warm(result, n)
                        record(dict(section="warm", n=n, block=block, engine=engine, **result))
                print(f"warm n={n} complete", flush=True)
        elif args.section == "lifecycle":
            cells = [("sessions", rows, count) for rows in [0, 1000, 10000] for count in [1, 16]]
            cells += [("history", rows, count) for rows in [1000, 10000, 100000] for count in [0, 100, 1000]]
            cells += [("host", size, 100) for size in [0, 1024, 65536]]
            cells += [("resource", limit, size) for limit, size in
                      [(262144, 1), (1048576, 1024), (1048576, 1048576),
                       (4194304, 1048576), (4194304, 4194304)]]
            for mode, n, count in cells:
                for block in range(-1, args.reps):
                    engines = ["main", "candidate"] if block % 2 else ["candidate", "main"]
                    for engine in engines:
                        result = invoke([str(MICRO / engine / "probe"), mode, str(n), str(count)])
                        record(dict(section="lifecycle", mode=mode, n=n, count=count,
                                    block=block, engine=engine, **result))
                print(f"{mode} {n} {count} complete", flush=True)
        elif args.section == "programs":
            # bench/*.py through `mm run`, checked against bench/expected.txt.
            # Boundary: Python subprocess spawn to exit (not xtask's).
            expected = dict(line.split(": ", 1) for line in
                            (ROOT / "bench/expected.txt").read_text().splitlines()
                            if line and not line.startswith("#"))
            variants = args.variants.split(",")
            # MICRO/<variant>/env (KEY=VALUE lines), if present, is added to the
            # environment: two variants can share one binary and differ only in it.
            envs = {}
            for variant in variants:
                path = MICRO / variant / "env"
                extra = dict(line.split("=", 1) for line in path.read_text().splitlines()
                             if line) if path.is_file() else {}
                envs[variant] = dict(os.environ, **extra)
            for program, want in expected.items():
                stem = "json_rows" if program == "json" else program
                for block in range(-1, args.reps):
                    order = variants[block % len(variants):] + variants[:block % len(variants)]
                    for variant in order:
                        result = invoke([MICRO / variant / "mm", "run", ROOT / "bench" / f"{stem}.py"],
                                        parse=False, env=envs[variant])
                        if result["outcome"] == "pass" and result["stdout"].strip() != want.strip():
                            result["outcome"] = "wrong_result"
                        record(dict(section="programs", program=program, block=block,
                                    engine=variant, **result))
                print(f"{program} complete", flush=True)
        else:
            for rows in [1000, 10000, 100000]:
                for changed in [0, max(1, rows // 100), rows]:
                    for block in range(-1, args.reps):
                        modes = ["full", "delta"] if block % 2 else ["delta", "full"]
                        for mode in modes:
                            directory = args.out / "images" / f"{rows}-{changed}-{mode}"
                            command = [str(MICRO / "persistence"), mode, str(rows), str(changed), str(directory)]
                            result = invoke(command)
                            if result["outcome"] == "pass":
                                try:
                                    result["compression"] = compression(directory)
                                except (OSError, AssertionError, zlib.error) as error:
                                    result.update(outcome="harness_error", error=repr(error))
                            if result["outcome"] == "pass":
                                result["fresh_process"] = invoke([
                                    str(MICRO / "persistence"), "restore", str(rows), str(changed), str(directory)])
                                if result["fresh_process"]["outcome"] != "pass":
                                    result["outcome"] = "fresh_restore_failure"
                            record(dict(section="persistence", rows=rows, changed=changed,
                                        block=block, mode=mode, **result))
                    print(f"persistence rows={rows} changed={changed} complete", flush=True)
    (args.out / "summary.json").write_text(json.dumps(summarize(samples), indent=2))
    print(f"{args.section}: {failures} failed observations (including warmups)", flush=True)
    return bool(failures)


if __name__ == "__main__":
    sys.exit(main())

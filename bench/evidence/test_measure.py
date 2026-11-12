"""Negative controls for measure.py: each failure kind must be classified
distinctly and must never receive a score. Run: python3 bench/evidence/test_measure.py
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

MEASURE = Path(__file__).with_name("measure.py")
# warm_ns median is 2000 ns but the mean is 4000: a mean would be caught.
GOOD = 'echo "{\\"checked_result\\":$(($2*($2-1)/2)),\\"warm_ns\\":[1000,9000,2000]}"'
WARM = {
    "layers/main/probe": GOOD,
    "layers/candidate/probe": 'echo "{\\"checked_result\\":$(($2*($2-1)/2+1)),\\"warm_ns\\":[1,1,1]}"',
    "baselines/bin/numeric-qjs": "echo 'TypeError: boom' >&2; exit 1",
    "baselines/bin/numeric-dquickts": "kill -SEGV $$",
    "baselines/bin/numeric-qpy": "exec sleep 30",
    "baselines/bin/monty-warm": "echo 'not json'",
    "djs/probe-target/release/djs-probe": "true",
    "python": 'echo "{\\"checked_result\\":$(($2*($2-1)/2)),\\"warm_ns\\":[1,1]}"',
}
EXPECTED = {"main": "pass", "candidate": "wrong_result", "qjs": "error", "dquickts": "crash",
            "qpy": "timeout", "monty": "harness_error", "djs": "harness_error",
            "cpython": "harness_error"}


def fixture(prefix, relative, body):
    path = Path(f"{prefix}-{relative}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"#!/bin/sh\n{body}\n")
    path.chmod(0o755)
    return path


def run(prefix, section, out, *extra):
    env = dict(os.environ, MM_EVIDENCE_PREFIX=str(prefix), PYTHON=f"{prefix}-python")
    return subprocess.run([sys.executable, MEASURE, section, out, "--reps", "2", *extra],
                          capture_output=True, text=True, env=env, timeout=300)


def lines(path):
    return [json.loads(line) for line in path.read_text().splitlines()]


class NegativeControls(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.prefix = Path(self.dir.name) / "e"
        self.out = Path(self.dir.name) / "out"

    def tearDown(self):
        self.dir.cleanup()

    def test_each_failure_kind_is_distinct_and_unscored(self):
        for relative, body in WARM.items():
            fixture(self.prefix, relative, body)
        result = run(self.prefix, "warm", self.out, "--timeout", "1")
        self.assertEqual(result.returncode, 1, result.stderr)
        samples = lines(self.out / "samples.jsonl")
        self.assertEqual(len(samples), 3 * 3 * len(EXPECTED), "3 sizes x (warmup + 2 reps)")
        for engine, outcome in EXPECTED.items():
            seen = {s["outcome"] for s in samples if s["engine"] == engine}
            self.assertEqual(seen, {outcome}, engine)
        summary = json.loads((self.out / "summary.json").read_text())
        scored = {row["engine"] for row in summary if "median_warm_ms" in row}
        self.assertEqual(scored, {"main"})
        for row in summary:
            if row["engine"] == "main":
                self.assertEqual(row["median_warm_ms"], 0.002)
                self.assertEqual(row["rounds"], 2)
            else:
                self.assertNotIn("median_process_ms", row)

    def test_a_failure_after_passing_rounds_removes_the_score(self):
        for relative, body in WARM.items():
            fixture(self.prefix, relative, GOOD)
        state = Path(self.dir.name) / "calls"
        # Passes the outer warmup, then fails only its second timed round at n=1000.
        fixture(self.prefix, "layers/candidate/probe",
                f'echo x >> {state}; [ "$2" = 1000 ] && [ $(wc -l < {state}) -eq 3 ] && exit 1; {GOOD}')
        result = run(self.prefix, "warm", self.out)
        self.assertEqual(result.returncode, 1, result.stderr)
        summary = json.loads((self.out / "summary.json").read_text())
        flaky = [r for r in summary if r["engine"] == "candidate" and r["n"] == 1000]
        self.assertEqual(flaky[0]["outcomes"], {"pass": 2, "error": 1})
        self.assertNotIn("median_warm_ms", flaky[0])
        scored = [r for r in summary if "median_warm_ms" in r]
        self.assertEqual(len(scored), 3 * len(EXPECTED) - 1)

    def test_missing_engine_fails_before_any_output(self):
        for relative, body in WARM.items():
            if relative != "baselines/bin/numeric-qpy":
                fixture(self.prefix, relative, GOOD)
        result = run(self.prefix, "warm", self.out)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("numeric-qpy", result.stderr)
        self.assertFalse(self.out.exists())

    def test_programs_check_every_checksum(self):
        expected = dict(line.split(": ", 1) for line in
                        (MEASURE.parents[2] / "bench/expected.txt").read_text().splitlines()
                        if line and not line.startswith("#"))
        cases = "".join(f"*/{'json_rows' if name == 'json' else name}.py) echo '{want}';;\n"
                        for name, want in expected.items())
        # "bad" is the good binary; only its env file makes fib wrong.
        fixture(self.prefix, "layers/good/mm",
                f'[ "$BROKEN" = 1 ] && case "$2" in */fib.py) echo 1; exit;; esac\n'
                f'case "$2" in\n{cases}esac')
        bad = Path(f"{self.prefix}-layers/bad")
        bad.mkdir(parents=True)
        (bad / "mm").symlink_to(Path(f"{self.prefix}-layers/good/mm"))
        (bad / "env").write_text("BROKEN=1\n")
        result = run(self.prefix, "programs", self.out, "--variants", "good,bad")
        self.assertEqual(result.returncode, 1, result.stderr)
        samples = lines(self.out / "samples.jsonl")
        self.assertEqual(len(samples), len(expected) * 3 * 2)
        bad = {s["outcome"] for s in samples if s["engine"] == "bad" and s["program"] == "fib"}
        self.assertEqual(bad, {"wrong_result"})
        rest = {s["outcome"] for s in samples if s["engine"] == "good" or s["program"] != "fib"}
        self.assertEqual(rest, {"pass"})
        summary = json.loads((self.out / "summary.json").read_text())
        self.assertEqual(len([r for r in summary if "median_process_ms" in r]),
                         2 * len(expected) - 1)

    def test_persistence_restore_and_image_failures(self):
        # Writes nine images for FULL; delta writes none; restore always fails.
        fixture(self.prefix, "layers/persistence", """case "$1" in
restore) echo 'restore failed' >&2; exit 1;;
full) mkdir -p "$4"; for i in 0 1 2 3 4 5 6 7 8; do echo image > "$4/$i.mmsnap"; done;;
esac
echo '{"ok":true}'""")
        result = run(self.prefix, "persistence", self.out)
        self.assertEqual(result.returncode, 1, result.stderr)
        samples = lines(self.out / "samples.jsonl")
        self.assertEqual({s["outcome"] for s in samples if s["mode"] == "full"},
                         {"fresh_restore_failure"})
        self.assertEqual({s["outcome"] for s in samples if s["mode"] == "delta"},
                         {"harness_error"})
        full = next(s for s in samples if s["mode"] == "full")
        self.assertEqual(full["compression"][0]["raw_bytes"], len("image\n"))
        summary = json.loads((self.out / "summary.json").read_text())
        self.assertFalse([r for r in summary if "median_process_ms" in r])


if __name__ == "__main__":
    unittest.main()

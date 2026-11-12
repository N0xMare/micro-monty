"""One correctness run per read-only bench file; no clocks or timing rounds."""

import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--python", required=True)
    parser.add_argument("--qjs", required=True)
    parser.add_argument("--monty", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    expected = dict(
        line.split(": ", 1)
        for line in (args.repo / "bench/expected.txt").read_text().splitlines()
        if line and not line.startswith("#")
    )
    rows = []
    for engine, executable in (
        ("cpython", args.python), ("stock-quickjs", args.qjs), ("monty", args.monty)
    ):
        for name, want in expected.items():
            stem = "json_rows" if name == "json" else name
            suffix = ".js" if engine == "stock-quickjs" else ".py"
            command = [executable, str(args.repo / "bench" / (stem + suffix))]
            try:
                result = subprocess.run(
                    command, capture_output=True, text=True, timeout=30, cwd=args.repo
                )
                row = dict(
                    engine=engine, program=name, command=command,
                    returncode=result.returncode, stdout=result.stdout,
                    stderr=result.stderr, expected=want,
                )
                row["status"] = (
                    "pass" if result.returncode == 0 and result.stdout.strip() == want
                    else "error" if result.returncode else "checksum_mismatch"
                )
            except subprocess.TimeoutExpired:
                row = dict(engine=engine, program=name, command=command, status="timeout")
            rows.append(row)
            print(engine, name, row["status"], flush=True)
    args.output.write_text(json.dumps(rows, indent=2) + "\n")
    # Expected gaps are asserted, not silently counted as passing checksums.
    for row in rows:
        if row["engine"] == "monty" and row["program"] in ("generators", "exceptions"):
            required = (
                "does not yet support yield expressions"
                if row["program"] == "generators"
                else "does not yet support class inheritance and metaclasses"
            )
            assert row["status"] == "error" and required in row["stderr"], row
        else:
            assert row["status"] == "pass", row


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Grades the two prompts with one checkable answer, on the review worktree's
commit (1f27970): `todo-census` (there are no TODO, FIXME or XXX comments
in crates/; the four `XXX` hits are `\\uXXXX` strings) and `loc-census`
(mm-engine/src is the largest crate, 51,988 lines).

    python3 grade.py RAW_DIR
"""

import json
import re
import sys
from pathlib import Path

for run_file in sorted(Path(sys.argv[1]).glob("*/*/run.json")):
    run = json.loads(run_file.read_text())
    if run["prompt"] not in ("todo-census", "loc-census"):
        continue
    text = "\n".join(l for l in (run_file.parent / "stdout.txt").read_text().splitlines()
                     if not l.startswith("session_id="))
    if run["prompt"] == "todo-census":
        ok = bool(re.search(r"\bno\b[^.\n]{0,40}(TODO|comments)|\*\*0\*\*|total[^0-9\n]{0,20}\b0\b",
                            text, re.I))
    else:
        ok = bool(re.search(r"51,?988", text))
    print(json.dumps({"arm": run["arm"], "prompt": run["prompt"], "exit": run["exit"],
                      "correct": ok}))

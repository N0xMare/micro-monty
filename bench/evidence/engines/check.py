"""Checks the tool calls an engine made against the real codemode checker.

    python3.14 bench/evidence/engines/check.py CALLS.jsonl

Each line of CALLS.jsonl is `{"engine", "task", "calls": [[name, args], ...]}`
in the order the engine issued them (a host's `workload --calls` output). The
calls are replayed through the task's real mocked tools (ptc.ToolHost, which
also keeps the retry tools' state) and the task's checker judges the
transcript, as conformance/codemode/run.py does for CPython and micro-monty.
Prints one verdict per line and exits nonzero if any task fails.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2] / 'conformance' / 'codemode'))

import ptc  # noqa: E402
import tasks  # noqa: E402,F401  (registers every task)


def main() -> int:
    bad = 0
    for line in Path(sys.argv[1]).read_text().splitlines():
        rec = json.loads(line)
        task = ptc.TASKS[rec['task']]
        host = ptc.ToolHost(task, 'structured')
        for name, args in rec['calls']:
            host.handle(name, [args], [])
        problem = rec.get('error') or task.check(host.transcript)
        verdict = 'pass' if problem is None else f'FAIL {problem}'
        bad += problem is not None
        print(f'{rec["engine"]:14} {rec["task"]:26} {verdict}')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())

#!/usr/bin/env python3
"""Replays every recorded plak cell on the current micro-monty: a regression gate.

    python3 conformance/codemode/real/gate.py            # replay, rewrite results/gate.jsonl
    python3 conformance/codemode/real/gate.py --check    # fail if any cell moved
    python3 conformance/codemode/real/gate.py --filter grok-sdk --check

`traces.tar.xz` holds the 160 recorded sessions of Phase 1 and Phase 2 as
the tracer wrote them (README "Regression gate"). Each session is replayed
in its recorded order on one worker, `mm-cell-worker`'s `replay_worker`
example: the shipped protocol loop, codec and session settings over stdio,
with the wall clock pinned to the session's recorded start and entropy a
counter. The script sends the recorded `Configure`, `Reset` and
`StartCell` frames and answers the k-th nested call of a cell with the
recorded result of the cell's k-th call once the worker reports it
`Waiting`. A call whose name or input differs from the recording diverges
the session: its later cells are not run.

Per cell it compares, against the recording: the nested calls (name and
input), the batches (`Waiting`), the delivery verdicts, the printed output,
and the whole `CellDone` outcome (kind, value, exception message with its
traceback, stored names). `results/gate.jsonl` has one record per cell, in
session order, holding only the fields that differ from the recording, with
their current value, and a `why` for each cell recorded on micro-monty. The
expected result of a cell is the recording with that record's fields laid
over it; `--check` fails on any difference and writes nothing. Without
`--check` the file is rewritten, keeping each `why` whose cell still
differs the same way; commit it to record an intended change.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import difflib
import json
import os
import struct
import subprocess
import sys
import tarfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent.parent
ARCHIVE = HERE / 'traces.tar.xz'
EXPECTED = HERE / 'results' / 'gate.jsonl'
FIELDS = ('calls', 'batches', 'deliveries', 'output', 'outcome')
TIMEOUT = 120


def sessions(only: str | None):
    """(id, clock, trace lines) per recorded session, in id order."""
    traces = {}
    with tarfile.open(ARCHIVE) as tar:
        for member in tar:
            # mm-real-raw/<phase>/<arm>/<prompt>/trace-*.jsonl (and run.json)
            parts = member.name.split('/')
            if not member.isfile() or not parts[-1].startswith('trace-'):
                continue
            sid = '/'.join(parts[1:4])
            if sid in traces:
                raise SystemExit(f'{sid}: more than one trace')
            traces[sid] = tar.extractfile(member).read().decode()
    for sid in sorted(traces):
        if only and only not in sid:
            continue
        lines = [json.loads(line) for line in traces[sid].splitlines()]
        start = next(item['unix_ns'] for item in lines if item['d'] == 's')
        yield sid, start / 1e9, lines


def recorded_cells(lines):
    """The host frames to send, and per cell what the recorded worker did."""
    script, cells, by_call = [], {}, {}
    for item in lines:
        frame = item.get('f')
        if not frame:
            continue
        name = frame['frame']
        if item['d'] == 'h':
            if name == 'nested_result':
                by_call[frame['call']]['answer'] = frame['outcome']
            elif name in ('configure', 'reset', 'start_cell', 'shutdown'):
                script.append(frame)
            else:
                raise SystemExit(f'unexpected host frame {name}')
            if name == 'start_cell':
                cells[frame['cell']] = {'calls': [], 'answers': [], 'batches': [],
                                        'deliveries': [], 'output': '', 'outcome': None}
            continue
        cell = cells.get(frame.get('cell'))
        if cell is None:
            continue
        if name == 'nested_call':
            call = {'name': frame['name'], 'input': frame['input']}
            cell['calls'].append([frame['name'], frame['input']])
            cell['answers'].append(call)
            by_call[frame['call']] = call
        elif name == 'waiting':
            cell['batches'].append(len(frame['calls']))
        elif name == 'delivery':
            cell['deliveries'].append(frame['verdict']['verdict'])
        elif name == 'output':
            cell['output'] += frame['item'].get('text', '')
        elif name == 'cell_done':
            cell['outcome'] = frame['outcome']
        else:
            raise SystemExit(f'unexpected worker frame {name} (yields and cancels are not replayed)')
    return script, cells


class Worker:
    def __init__(self, path, clock):
        env = {'PATH': '/usr/bin:/bin', 'MM_REPLAY_CLOCK': repr(clock)}
        self.process = subprocess.Popen([path], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, env=env)

    def send(self, frame):
        body = json.dumps(frame).encode()
        self.process.stdin.write(struct.pack('>I', len(body)) + body)
        self.process.stdin.flush()

    def recv(self):
        head = self.process.stdout.read(4)
        if len(head) < 4:
            return None
        (length,) = struct.unpack('>I', head)
        return json.loads(self.process.stdout.read(length))


def replay_cell(worker, start, recorded):
    """What the worker does for one cell, and why it diverged (or None)."""
    seen = {'calls': [], 'batches': [], 'deliveries': [], 'output': '', 'outcome': None}
    pending = {}
    worker.send(start)
    while True:
        frame = worker.recv()
        if frame is None:
            seen['outcome'] = {'outcome': 'lost', 'message': 'the worker exited'}
            return seen, 'the worker exited'
        name = frame['frame']
        if name == 'host_lost':
            seen['outcome'] = {'outcome': 'lost', 'message': frame['message']}
            return seen, 'the worker was lost'
        if name == 'output':
            seen['output'] += frame['item'].get('text', '')
        elif name == 'nested_call':
            k = len(seen['calls'])
            seen['calls'].append([frame['name'], frame['input']])
            if k >= len(recorded['answers']):
                return seen, f'call {k + 1} ({frame["name"]}) was not recorded'
            want = recorded['answers'][k]
            if (want['name'], want['input']) != (frame['name'], frame['input']):
                return seen, f'call {k + 1}: {frame["name"]} where {want["name"]} was recorded'
            if 'answer' not in want:
                return seen, f'call {k + 1} has no recorded result'
            pending[frame['call']] = want['answer']
        elif name == 'waiting':
            seen['batches'].append(len(frame['calls']))
            for call in frame['calls']:
                if call not in pending:
                    return seen, f'waiting on unknown call {call}'
                worker.send({'frame': 'nested_result', 'cell': start['cell'], 'call': call,
                             'outcome': pending.pop(call)})
        elif name == 'delivery':
            seen['deliveries'].append(frame['verdict']['verdict'])
        elif name == 'cell_done':
            seen['outcome'] = frame['outcome']
            return seen, None
        else:
            return seen, f'unexpected frame {name}'


def replay_session(path, session):
    """One record per recorded cell: the fields that differ from the recording."""
    sid, clock, lines = session
    script, cells = recorded_cells(lines)
    worker = Worker(path, clock)
    records, broken = [], None
    try:
        if (worker.recv() or {}).get('frame') != 'hello':
            raise SystemExit(f'{sid}: no hello from {path}')
        for frame in script:
            name = frame['frame']
            if name == 'start_cell':
                record = {'cell': f'{sid}/{frame["cell"]}'}
                recorded = cells[frame['cell']]
                if broken:
                    record['skipped'] = broken
                else:
                    seen, diverged = replay_cell(worker, frame, recorded)
                    if diverged:
                        record['diverged'] = diverged
                        broken = f'{sid}/{frame["cell"]} diverged'
                    for field in FIELDS:
                        if seen[field] != recorded[field]:
                            record[field] = seen[field]
                records.append(record)
            elif not broken:
                worker.send(frame)
        if broken:
            worker.process.kill()
        worker.process.stdin.close()
        worker.process.wait(timeout=TIMEOUT)
    finally:
        if worker.process.poll() is None:
            worker.process.kill()
    return sid, records, lines


def busy(session) -> int:
    """The time the recorded cells spent outside host calls, in trace units."""
    total, mark = 0, 0
    for item in session[2]:
        if item['d'] == 'w':
            total += item['t'] - mark
        if item['d'] in ('h', 'w'):
            mark = item['t']
    return total


def build() -> str:
    subprocess.run(['cargo', 'build', '--release', '--locked', '--quiet', '-p', 'mm-cell-worker',
                    '--example', 'replay_worker'], cwd=ROOT, check=True)
    return str(ROOT / 'target' / 'release' / 'examples' / 'replay_worker')


def differences(record: dict) -> list[str]:
    return [key for key in record if key not in ('cell', 'why')]


def lines(outcome) -> str:
    """A `CellDone` outcome as text, its traceback one line per line."""
    text = []
    for key, value in (outcome or {}).items():
        if isinstance(value, str) and '\n' in value:
            text += [f'{key}:', *value.splitlines()]
        else:
            text.append(f'{key}: {json.dumps(value)}')
    return '\n'.join(text)


def show(record: dict, expected: dict, recorded: dict):
    """The fields of a cell that changed, as diffs where they are text."""
    for field in sorted(set(differences(record)) | set(differences(expected))):
        old = expected.get(field, recorded.get(field))
        new = record.get(field, recorded.get(field))
        if old == new:
            continue
        print(f'  {field}:')
        if field == 'outcome':
            old, new = lines(old), lines(new)
        if isinstance(old, str) and isinstance(new, str):
            diff = difflib.unified_diff(old.splitlines(), new.splitlines(), 'expected', 'now',
                                        lineterm='', n=1)
            for line in list(diff)[:40]:
                print(f'    {line}')
        else:
            print(f'    expected {json.dumps(old)[:400]}')
            print(f'    now      {json.dumps(new)[:400]}')


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--check', action='store_true', help='write nothing; fail if any cell moved')
    ap.add_argument('--filter', help='only sessions whose id contains this')
    ap.add_argument('--jobs', type=int, default=os.cpu_count() or 4)
    ap.add_argument('--worker', help='a replay worker to use instead of building one')
    args = ap.parse_args()
    began = time.monotonic()
    path = args.worker or build()
    expected = {}
    if EXPECTED.exists():
        for line in EXPECTED.read_text().splitlines():
            record = json.loads(line)
            expected[record['cell']] = record
    # Longest recorded sessions first, results in id order.
    todo = sorted(sessions(args.filter), key=busy, reverse=True)
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        results = sorted(pool.map(lambda s: replay_session(path, s), todo))
    elapsed = time.monotonic() - began

    changed, cells, differing, diverged = [], 0, 0, 0
    for _, records, lines in results:
        _, recorded = recorded_cells(lines)
        for record in records:
            cells += 1
            old = expected.get(record['cell'], {'cell': record['cell']})
            if differences(record):
                differing += 1
                diverged += 'diverged' in record or 'skipped' in record
                if 'why' in old and differences(old) == differences(record) and \
                        all(old[k] == record[k] for k in differences(record)):
                    record['why'] = old['why']
            if {k: v for k, v in record.items() if k != 'why'} != \
                    {k: v for k, v in old.items() if k != 'why'}:
                changed.append((record, old, recorded[int(record['cell'].rsplit('/', 1)[1])]))
    seen_ids = {r['cell'] for _, records, _ in results for r in records}
    missing = [cell for cell in expected if cell not in seen_ids and
               (not args.filter or args.filter in cell)]

    for record, old, recorded in changed:
        print(f'{record["cell"]}: {", ".join(differences(record) or ["now as recorded"])}')
        show(record, old, recorded)
    for cell in missing:
        print(f'{cell}: expected but not in the archive')
    print(f'{cells} cells in {len(results)} sessions, {elapsed:.1f} s: {differing} differ from '
          f'the recording ({diverged} diverged or skipped), {len(changed)} changed since '
          f'results/gate.jsonl')
    if args.check:
        if changed or missing:
            print('FAIL: replayed cells differ from results/gate.jsonl; rerun without --check '
                  'and commit it if the change is intended')
            return 1
        return 0
    if args.filter:
        print('not written: --filter replays a subset')
        return 0
    with EXPECTED.open('w') as out:
        for _, records, _ in results:
            for record in records:
                out.write(json.dumps(record, ensure_ascii=False) + '\n')
    print(f'wrote {EXPECTED.relative_to(ROOT)}')
    return 0


if __name__ == '__main__':
    sys.exit(main())

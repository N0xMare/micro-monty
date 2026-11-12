"""Runs both codemode corpora on a build with another default module set.

    python3.14 -I bench/evidence/stdlib_profile/behaviour.py OUT.json LABEL=mod,mod,... [...]
        [--jobs N] [--first-worktree K]

For each LABEL it takes a git worktree of HEAD (`/home/user/workspace/wtNN`,
from `--first-worktree`), sets the facade's `default` list to the given
modules (closed over the feature graph by Cargo; `baseline` means the
unchanged list), builds `mm-cell-worker`'s `replay_worker` and `mm-codemode`
in release as the two gates do, and runs:

- real: every recorded plak cell through conformance/codemode/real/gate.py's
  replay (`replay_session`). A cell's outcome under the build is the
  recording with the replay's differing fields laid over it.
- synthetic: every census solution (not the probes) through
  conformance/codemode/run.py's runner (`run_all`), CPython included, as
  the `mm-default` engine.

It compares each cell and solution with the committed expectation of the
default build (`results/gate.jsonl`, `results/runs.jsonl` `mm-default`) and
records, per label, the units that changed at all and the units whose
outcome went from success to failure (real: `completed` to anything else;
synthetic: `pass` to anything else), with their first cause.
"""
import argparse
import concurrent.futures
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import features  # noqa: E402
from removals import set_default  # noqa: E402

ROOT = features.ROOT
sys.path.insert(0, str(ROOT / 'conformance/codemode/real'))
sys.path.insert(0, str(ROOT / 'conformance/codemode'))
import gate  # noqa: E402
import run as codemode  # noqa: E402


def worktree(i):
    wt = Path(f'/home/user/workspace/wt{i:02d}')
    assert len(str(wt)) == len(str(ROOT.resolve()))
    if not wt.exists():
        subprocess.run(['git', 'worktree', 'add', '--detach', str(wt), 'HEAD'], cwd=ROOT,
                       check=True, capture_output=True)
    return wt


def build(wt, mods):
    set_default(wt, mods)
    env = dict(os.environ)
    env.pop('CARGO_TARGET_DIR', None)
    subprocess.run(['cargo', 'build', '--release', '--locked', '--quiet',
                    '-p', 'mm-cell-worker', '--example', 'replay_worker'],
                   cwd=wt, env=env, check=True)
    subprocess.run(['cargo', 'build', '--release', '--locked', '--quiet', '-p', 'mm-codemode'],
                   cwd=wt, env=env, check=True)
    rel = wt / 'target/release'
    return rel / 'examples/replay_worker', rel / 'mm-codemode'


def real(worker, jobs):
    expected = {}
    for line in gate.EXPECTED.read_text().splitlines():
        r = json.loads(line)
        expected[r['cell']] = r
    sessions = sorted(gate.sessions(None), key=gate.busy, reverse=True)
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        results = list(pool.map(lambda s: gate.replay_session(str(worker), s), sessions))
    changed, broke, n = [], [], 0
    for _, records, lines in results:
        _, recorded = gate.recorded_cells(lines)
        for record in records:
            n += 1
            old = expected[record['cell']]
            strip = lambda r: {k: v for k, v in r.items() if k != 'why'}  # noqa: E731
            if strip(record) == strip(old):
                continue
            rec = recorded[int(record['cell'].rsplit('/', 1)[1])]
            before = old.get('outcome', rec['outcome']) or {}
            after = record.get('outcome', rec['outcome']) or {}
            if 'skipped' in record:
                after = {'outcome': 'skipped'}
            item = {'cell': record['cell'], 'fields': gate.differences(record),
                    'before': before.get('outcome'), 'after': after.get('outcome'),
                    'message': (after.get('message') or after.get('error') or '')[:300]}
            changed.append(item)
            if before.get('outcome') == 'completed' and after.get('outcome') != 'completed':
                broke.append(item)
    return {'units': n, 'changed': len(changed), 'completed_to_failed': len(broke),
            'changed_units': changed}


def synthetic(binary, jobs):
    committed = {}
    for line in (ROOT / 'conformance/codemode/results/runs.jsonl').read_text().splitlines():
        r = json.loads(line)
        committed[r['set'], r['task']] = r
    items = [i for i in codemode.solutions(None) if i[0] != 'p']
    keys = ('outcome', 'cause', 'error', 'problem', 'same_calls', 'same_stdout', 'stdout_bytes',
            'max_waiting')
    records = [{'set': r['set'], 'task': r['task'],
                'mm-default': {k: r['mm-default'].get(k) for k in keys
                               if r['mm-default'].get(k) is not None}}
               for r in codemode.run_all({'default': binary}, items, jobs)]
    changed, broke = [], []
    for r in records:
        old = committed[r['set'], r['task']]['mm-default']
        new = r['mm-default']
        if new == old:
            continue
        item = {'unit': f"{r['set']}/{r['task']}", 'before': old.get('outcome'),
                'after': new.get('outcome'), 'cause': new.get('cause')}
        changed.append(item)
        if old.get('outcome') == 'pass' and new.get('outcome') != 'pass':
            broke.append(item)
    return {'units': len(records), 'changed': len(changed), 'pass_to_fail': len(broke),
            'changed_units': changed}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('out')
    ap.add_argument('configs', nargs='+')
    ap.add_argument('--jobs', type=int, default=os.cpu_count() or 4)
    ap.add_argument('--first-worktree', type=int, default=9)
    a = ap.parse_args()
    out = Path(a.out)
    results = json.loads(out.read_text()) if out.exists() else {}
    wt = worktree(a.first_worktree)
    for spec in a.configs:
        label, _, mods = spec.partition('=')
        if label in results:
            continue
        mods = list(features.DEFAULT) if label == 'baseline' else [m for m in mods.split(',') if m]
        worker, binary = build(wt, mods)
        r = {'modules': mods, 'real': real(worker, a.jobs), 'synthetic': synthetic(binary, a.jobs)}
        results[label] = r
        out.write_text(json.dumps(results, indent=1))
        print(label, 'real', r['real']['changed'], r['real']['completed_to_failed'],
              'synthetic', r['synthetic']['changed'], r['synthetic']['pass_to_fail'], flush=True)


if __name__ == '__main__':
    main()

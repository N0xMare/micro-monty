"""Interleaved A/B wall-time comparison of `mm run PROGRAM` for two builds.
usage: ab.py REPS OUT.json NAME=MM NAME=MM -- PROGRAM.py ...
Each rep runs every (build, program) once in a shuffled order; 3 warm-up reps
are discarded. Checks the printed line against bench/expected.txt."""
import json, os, random, statistics, subprocess, sys, time

reps, out = int(sys.argv[1]), sys.argv[2]
sep = sys.argv.index('--')
builds = dict(a.split('=', 1) for a in sys.argv[3:sep])
progs = sys.argv[sep + 1:]
root = os.path.dirname(os.path.dirname(os.path.abspath(progs[0])))
expected = {}
for line in open(os.path.join(root, 'bench/expected.txt')):
    if line.strip() and not line.startswith('#'):
        expected[line.split(':')[0]] = line.strip()
times = {(b, p): [] for b in builds for p in progs}
rng = random.Random(1)
for rep in range(reps + 3):
    jobs = [(b, p) for b in builds for p in progs]
    rng.shuffle(jobs)
    for b, p in jobs:
        t = time.perf_counter()
        r = subprocess.run([builds[b], 'run', p], capture_output=True, text=True)
        dt = time.perf_counter() - t
        name = os.path.basename(p)[:-3]
        want = expected.get('json' if name == 'json_rows' else name)
        if r.returncode or (want and r.stdout.strip() != want.split(': ', 1)[1]):
            sys.exit(f'{b} {p}: rc {r.returncode} out {r.stdout.strip()!r} want {want!r} {r.stderr[-300:]}')
        if rep >= 3:
            times[(b, p)].append(dt * 1000)
res = {}
names = list(builds)
for p in progs:
    row = {}
    for b in names:
        v = sorted(times[(b, p)])
        q = statistics.quantiles(v, n=4)
        row[b] = {'median_ms': statistics.median(v), 'q1': q[0], 'q3': q[2], 'min': v[0], 'n': len(v)}
    a = names[0]
    for b in names[1:]:
        # Bootstrap 95% interval of the median ratio b/a.
        boot = []
        for _ in range(2000):
            sa = [rng.choice(times[(a, p)]) for _ in times[(a, p)]]
            sb = [rng.choice(times[(b, p)]) for _ in times[(b, p)]]
            boot.append(statistics.median(sb) / statistics.median(sa))
        boot.sort()
        row[f'ratio_{b}'] = row[b]['median_ms'] / row[a]['median_ms']
        row[f'ratio_{b}_ci95'] = [boot[50], boot[1949]]
    res[os.path.basename(p)] = row
    cols = '  '.join(f"{b} {row[b]['median_ms']:.1f} [{row[b]['q1']:.1f}-{row[b]['q3']:.1f}]" for b in names)
    rats = '  '.join(f"{b}/{a} {row[f'ratio_{b}']:.3f} ({row[f'ratio_{b}_ci95'][0]:.3f}-{row[f'ratio_{b}_ci95'][1]:.3f})" for b in names[1:])
    print(f"{os.path.basename(p):16} {cols}  {rats}", flush=True)
json.dump({'reps': reps, 'builds': builds, 'results': res, 'raw': {f'{b}|{os.path.basename(p)}': times[(b, p)] for b, p in times}},
          open(out, 'w'), indent=1)

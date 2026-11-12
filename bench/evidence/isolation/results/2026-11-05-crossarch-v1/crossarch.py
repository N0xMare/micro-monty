"""Cross-machine snapshot chains on the mm-wasm v1 head (README.md next to
this file). Every hop is a new process; hops alternate between this Mac
(aarch64 macOS) and host1 (x86-64 Linux, over ssh), and the working
directory (images, state) is copied each hop.

  crossarch.py own ENGINES OUT     own cases through `mm exec/resume`, parked about
                                   MAX times; ENGINES: native | wasip1 | mixed
  crossarch.py xchain OUT          the host-API scenario (iso-harness xchain) with every
                                   rotation of native / abi (wasip1) / abi-uu over both machines

Configuration (environment): MM, MM_WASM (precompiled .cwasm), WASMTIME,
HARNESS, ISO_ROOT for this Mac; R_MM, R_MM_WASM, R_WASMTIME, R_HARNESS,
R_ISO_ROOT for host1. ONLY=<substring> narrows the own cases."""
import glob, itertools, json, os, shlex, shutil, subprocess, sys, tempfile

MAX = 6
SSH = ['ssh', '-o', 'ControlMaster=auto', '-o', 'ControlPath=/tmp/ssh-%r@%h-%p', '-o', 'ControlPersist=600', 'host1']
RSH = ' '.join(SSH[:-1])
E = os.environ


def sync(src, dst):
    subprocess.run(['rsync', '-a', '-I', '--delete', '-e', RSH, src + '/', dst + '/'], check=True)


def run(on_remote, argv, cwd, env=None):
    """argv on this Mac or on host1 (cwd copied there and back); last stdout line as JSON."""
    env = env or {}
    if on_remote:
        sync(cwd, f'host1:{cwd}')
        cmd = f'cd {cwd} && ' + ' '.join(f'{k}={shlex.quote(v)}' for k, v in env.items()) + ' ' + shlex.join(argv)
        p = subprocess.run([*SSH, cmd], capture_output=True, timeout=600)
        sync(f'host1:{cwd}', cwd)
    else:
        p = subprocess.run(argv, cwd=cwd, env=dict(E, **env), capture_output=True, timeout=600)
    lines = p.stdout.decode().strip().splitlines()
    if not lines:
        raise RuntimeError(f'no output (rc {p.returncode}): {p.stderr.decode()[-500:]}')
    return json.loads(lines[-1])


def mm_argv(remote, wasm, scratch, args):
    if not wasm:
        return [E['R_MM'] if remote else E['MM'], *args]
    # As conformance/oracle.py wasm_command: the scratch directory preopened
    # at its own path, the case file absolute.
    cmd = [E['R_WASMTIME'] if remote else E['WASMTIME'], 'run', '--dir', f'{scratch}::{scratch}',
           '--dir', '/usr/share/zoneinfo::/usr/share/zoneinfo', '--env', 'TZ=UTC', '--allow-precompiled',
           E['R_MM_WASM'] if remote else E['MM_WASM']]
    return cmd + [os.path.join(scratch, a) if a.endswith('.py') else a for a in args]


def report(d):
    # Native mm on macOS resolves /tmp to /private/tmp; tracebacks carry the path.
    return json.loads(json.dumps({k: d.get(k) for k in ('stdout', 'result', 'error', 'exit')}).replace('/private/tmp/', '/tmp/'))


def scratch_dir():
    s = tempfile.mkdtemp(prefix='mmx-', dir='/tmp')
    subprocess.run([*SSH, f'mkdir -p {s}'], check=True)
    return s


def drop(s):
    shutil.rmtree(s, ignore_errors=True)
    subprocess.run([*SSH, f'rm -rf {s}'])


def own(engines, out):
    # Hop k's engine: native, wasip1, or (mixed) native/wasm alternating
    # every two hops, so each machine runs both.
    wasm_at = {'native': lambda k: False, 'wasip1': lambda k: True, 'mixed': lambda k: (k // 2) % 2 == 1}[engines]
    root = E['ISO_ROOT']
    results = []
    for start_remote in (False, True):
        for path in sorted(glob.glob(os.path.join(root, 'conformance/own/*' + E.get('ONLY', '') + '*.py'))):
            name = os.path.basename(path)
            scratch = scratch_dir()
            r = {'file': name, 'start': 'linux' if start_remote else 'mac'}
            try:
                shutil.copy(path, os.path.join(scratch, name))
                plain = run(False, [E['MM'], 'exec', '--json', '--report-fuel', '--main-module', name], scratch, {'TZ': 'UTC'})
                every = max(5, -(-plain.get('fuel', 0) // MAX))
                snap = os.path.join(scratch, 'snap')
                args = ['exec', '--json', '--main-module', '--snapshot-every', str(every), '--snapshot-dir', snap, name]
                hops = []
                while True:
                    k = len(hops)
                    on_remote = (k % 2 == 1) != start_remote
                    wasm = wasm_at(k)
                    hops.append(('linux' if on_remote else 'mac') + ('-wasm' if wasm else '-native'))
                    line = run(on_remote, mm_argv(on_remote, wasm, scratch, args), scratch, {'TZ': 'UTC'})
                    if 'park' not in line:
                        break
                    args = ['resume', '--json', snap]
                ok = report(line) == report(plain)
                r.update(parks=len(hops) - 1, hops=sorted(set(hops)), status='pass' if ok else 'fail')
                if not ok:
                    r['want'], r['got'] = report(plain), report(line)
            except Exception as e:  # noqa: BLE001
                r.update(status='error', reason=repr(e)[:300])
            finally:
                drop(scratch)
            results.append(r)
            print(json.dumps(r), flush=True)
    write(out, results)


def xchain(out):
    harness = lambda remote: [E['R_HARNESS'] if remote else E['HARNESS'], 'xchain']
    renv = lambda remote: {'ISO_ROOT': E['R_ISO_ROOT'] if remote else E['ISO_ROOT']}
    # The single-process references, on both machines and all engines, must agree.
    refs = {}
    for remote, engine in itertools.product((False, True), ('native', 'abi', 'abi-uu')):
        s = scratch_dir()
        try:
            refs[('linux' if remote else 'mac', engine)] = run(remote, harness(remote) + ['ref', engine], s, renv(remote))
        finally:
            drop(s)
    want = refs[('mac', 'native')]
    results = [{'chain': 'reference', 'engine': f'{m}/{e}', 'status': 'pass' if v == want else 'fail'} for (m, e), v in refs.items()]
    for r in results:
        print(json.dumps(r), flush=True)
    # Chains: one engine throughout (machines alternate), and every
    # rotation of the three engines, each starting on either machine.
    chains = [[e] for e in ('native', 'abi', 'abi-uu')] + [list(p) for p in itertools.permutations(('native', 'abi', 'abi-uu'))]
    for cycle, start_remote in itertools.product(chains, (False, True)):
        s = scratch_dir()
        hops = []
        r = {'chain': '>'.join(cycle), 'start': 'linux' if start_remote else 'mac'}
        try:
            while True:
                k = len(hops)
                remote = (k % 2 == 1) != start_remote
                engine = cycle[k % len(cycle)]
                hops.append(f"{'linux' if remote else 'mac'}/{engine}")
                line = run(remote, harness(remote) + ['step', engine, s], s, renv(remote))
                if 'hop' not in line:
                    break
                if k > 200:
                    raise RuntimeError('more than 200 hops')
            # `wanted` is fuel_wanted() read after each fuel park; a chain reads
            # it in the next hop, after a restore (finding in README.md).
            strip = lambda d: {k: v for k, v in d.items() if k != 'wanted'}
            ok = strip(line) == strip(want)
            r.update(hops=len(hops), combos=sorted(set(hops)), status='pass' if ok else 'fail',
                     wanted_match=line.get('wanted') == want['wanted'], wanted=line.get('wanted'))
            if not ok:
                r['want'], r['got'] = want, line
        except Exception as e:  # noqa: BLE001
            r.update(status='error', reason=repr(e)[:300], hops_done=hops)
        finally:
            drop(s)
        results.append(r)
        print(json.dumps(r), flush=True)
    results.append({'chain': 'reference-report', 'report': want})
    write(out, results)


def write(out, results):
    with open(out, 'w') as f:
        for r in results:
            f.write(json.dumps(r) + '\n')
    rows = [r for r in results if 'status' in r]
    print(f"{sum(r['status'] == 'pass' for r in rows)}/{len(rows)} pass")


if __name__ == '__main__':
    if sys.argv[1] == 'own':
        own(sys.argv[2], sys.argv[3])
    else:
        xchain(sys.argv[2])

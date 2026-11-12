"""Cross-architecture durability (results in crossarch.jsonl next to this
file): every own case parked about MAX times, the chain alternating between
native mm on this Mac (aarch64 macOS) and native mm on host1 (x86-64 Linux,
over ssh), the snapshot directory copied each hop. The final report must
equal an unparked run's. Usage: crossarch.py MM REMOTE_MM OUT"""
import json, os, subprocess, sys, shutil, tempfile, glob

MM, RMM, OUT = sys.argv[1:4]
MAX = 6
ENV = dict(os.environ, TZ='UTC')
SSH = ['ssh', '-o', 'ControlMaster=auto', '-o', 'ControlPath=/tmp/ssh-%r@%h-%p', '-o', 'ControlPersist=600', 'host1']


def local(args, cwd):
    p = subprocess.run([MM, *args], cwd=cwd, env=ENV, capture_output=True, timeout=120)
    return json.loads(p.stdout.decode().strip().splitlines()[-1])


def remote(args, cwd):
    subprocess.run(['rsync', '-a', '-I', '--delete', '-e', ' '.join(SSH[:-1]), cwd + '/', f'host1:{cwd}/'], check=True)
    cmd = f"cd {cwd} && TZ=UTC {RMM} " + ' '.join(f"'{a}'" for a in args)
    p = subprocess.run([*SSH, cmd], capture_output=True, timeout=120)
    subprocess.run(['rsync', '-a', '-I', '--delete', '-e', ' '.join(SSH[:-1]), f'host1:{cwd}/', cwd + '/'], check=True)
    return json.loads(p.stdout.decode().strip().splitlines()[-1])


def report(d):
    # The unparked run resolves /tmp to /private/tmp on macOS; tracebacks carry the path.
    return json.loads(json.dumps({k: d.get(k) for k in ('stdout', 'result', 'error', 'exit')}).replace('/private/tmp/', '/tmp/'))


results = []
root = subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], cwd=os.path.dirname(os.path.abspath(__file__)), text=True).strip()
for start_remote in (False, True):
    for path in sorted(glob.glob(os.path.join(root, 'conformance/own/*' + os.environ.get('ONLY', '') + '*.py'))):
        name = os.path.basename(path)
        scratch = tempfile.mkdtemp(prefix='mmx-', dir='/tmp')
        subprocess.run([*SSH, f'mkdir -p {scratch}'], check=True)
        try:
            shutil.copy(path, os.path.join(scratch, name))
            plain = local(['exec', '--json', '--report-fuel', '--main-module', name], scratch)
            every = max(5, -(-plain.get('fuel', 0) // MAX))
            args = ['exec', '--json', '--main-module', '--snapshot-every', str(every), '--snapshot-dir', 'snap', name]
            parks, hops = 0, []
            while True:
                on_remote = (parks % 2 == 1) != start_remote
                hops.append('linux' if on_remote else 'mac')
                line = (remote if on_remote else local)(args, scratch)
                if 'park' not in line:
                    break
                parks += 1
                args = ['resume', '--json', 'snap']
            ok = report(line) == report(plain)
            r = {'file': name, 'start': 'linux' if start_remote else 'mac', 'parks': parks,
                 'cross_hops': sum(a != b for a, b in zip(hops, hops[1:])), 'status': 'pass' if ok else 'fail'}
            if not ok:
                r['want'], r['got'] = report(plain), report(line)
        except Exception as e:  # noqa: BLE001
            r = {'file': name, 'start': 'linux' if start_remote else 'mac', 'status': 'error', 'reason': repr(e)[:300]}
        finally:
            shutil.rmtree(scratch, ignore_errors=True)
            subprocess.run([*SSH, f'rm -rf {scratch}'])
        results.append(r)
        print(json.dumps(r), flush=True)
with open(OUT, 'w') as f:
    for r in results:
        f.write(json.dumps(r) + '\n')
n = len(results)
p = sum(r['status'] == 'pass' for r in results)
print(f'{p}/{n} chains identical; {sum(r.get("cross_hops", 0) for r in results)} cross-architecture restores')

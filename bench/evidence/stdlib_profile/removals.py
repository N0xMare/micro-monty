"""Removal builds: the three size-budget artifacts with a changed module set.

    python3.14 -I bench/evidence/stdlib_profile/removals.py OUT.json [--jobs N] [--only LABEL,...]
        [--configs EXTRA.json]

For every default module `m`, the label `-m` builds with the facade's
`default` list minus `features.group(m)` (`m` and every default module whose
features pull it in), so the facade's feature graph is never edited. Labels
from `--configs` (`{"label": ["mod", ...]}`, a full module list) build with
exactly that list. `baseline` is the unchanged list.

Each build is what `cargo xtask size` ships (xtask/src/main.rs `shipped_cargo`
and `wasm::optimize`): `RUSTC_BOOTSTRAP=1`, `-Zbuild-std=std,panic_abort`,
`-Cpanic=immediate-abort -Cforce-unwind-tables=no`, release, `--locked`;
`hello` as `cargo rustc --example hello -- -Copt-level=3` for
wasm32-unknown-unknown and the host, the guest as `cargo build -p
mm-wasm-guest`; both wasm modules through `wasm-opt -Oz` with the shipped
feature list. Each job owns a git worktree of HEAD at
`/home/user/workspace/wtNN` (the same path length as the checkout, so
nothing path-dependent moves) with its own `target/`; only the facade's
`default` list in the worktree is rewritten. Results are written to OUT.json
after every build and reused on a rerun.
"""
import argparse
import concurrent.futures
import json
import os
import queue
import re
import subprocess
import sys
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import features  # noqa: E402

ROOT = features.ROOT
UU = 'wasm32-unknown-unknown'
NATIVE = 'x86_64-unknown-linux-gnu'
WASM_FEATURES = ['--enable-bulk-memory', '--enable-bulk-memory-opt',
                 '--enable-call-indirect-overlong', '--enable-multivalue',
                 '--enable-mutable-globals', '--enable-nontrapping-float-to-int',
                 '--enable-reference-types', '--enable-sign-ext']
WASM_OPT = os.environ.get('WASM_OPT') or str(Path.home() / '.local/bin/wasm-opt')
LOCK = threading.Lock()


def cargo(sub, target, wt, extra):
    env = dict(os.environ, RUSTC_BOOTSTRAP='1',
               RUSTFLAGS='-Zunstable-options -Cpanic=immediate-abort -Cforce-unwind-tables=no --cfg mm_shipped')
    env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    env.pop('CARGO_TARGET_DIR', None)
    cmd = ['cargo', sub, '--release', '--locked', '-Zbuild-std=std,panic_abort',
           '--target', target, *extra]
    p = subprocess.run(cmd, cwd=wt, env=env, capture_output=True, text=True)
    if p.returncode:
        raise RuntimeError(p.stderr[-3000:])


def wasm_opt(src, dst):
    subprocess.run([WASM_OPT, '-Oz', *WASM_FEATURES, str(src), '-o', str(dst)], check=True)
    return dst.stat().st_size


def set_default(wt, mods):
    path = wt / 'crates/micro-monty/Cargo.toml'
    text = (ROOT / 'crates/micro-monty/Cargo.toml').read_text()
    body = ''.join(f'    "mod-{m}",\n' for m in mods)
    new, n = re.subn(r'^default = \[\n.*?^\]\n', f'default = [\n{body}]\n', text,
                     count=1, flags=re.S | re.M)
    assert n == 1
    path.write_text(new)


def build(wt, mods):
    set_default(wt, mods)
    out = {}
    rel = wt / 'target' / UU / 'release'
    cargo('rustc', UU, wt, ['--package', 'micro-monty', '--example', 'hello', '--',
                            '-Copt-level=3'])
    out['hello.wasm'] = wasm_opt(rel / 'examples/hello.wasm', wt / 'target/hello.opt.wasm')
    cargo('rustc', NATIVE, wt, ['--package', 'micro-monty', '--example', 'hello', '--',
                                '-Copt-level=3'])
    out['native hello'] = (wt / 'target' / NATIVE / 'release/examples/hello').stat().st_size
    cargo('build', UU, wt, ['--package', 'mm-wasm-guest'])
    out['guest'] = wasm_opt(rel / 'mm_wasm_guest.wasm', wt / 'target/guest.opt.wasm')
    return out


def worktree(i):
    wt = Path(f'/home/user/workspace/wt{i:02d}')
    assert len(str(wt)) == len(str(ROOT.resolve())), (wt, ROOT)
    if not wt.exists():
        subprocess.run(['git', 'worktree', 'add', '--detach', str(wt), 'HEAD'], cwd=ROOT,
                       check=True, capture_output=True)
    return wt


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('out')
    ap.add_argument('--jobs', type=int, default=8)
    ap.add_argument('--only')
    ap.add_argument('--configs')
    a = ap.parse_args()
    configs = {'baseline': list(features.DEFAULT)}
    for m in features.DEFAULT:
        configs[f'-{m}'] = features.without(features.group(m))
    if a.configs:
        configs.update(json.loads(Path(a.configs).read_text()))
    if a.only:
        configs = {k: v for k, v in configs.items() if k in a.only.split(',')}
    out = Path(a.out)
    results = json.loads(out.read_text()) if out.exists() else {}
    todo = [k for k in configs if k not in results]
    # Every worker builds `baseline` first: it warms std and checks that the
    # worktree reproduces `cargo xtask size`.
    pool = queue.Queue()
    for i in range(1, a.jobs + 1):
        pool.put(worktree(i))

    def job(label):
        wt = pool.get()
        try:
            r = build(wt, configs[label])
        except Exception as e:  # noqa: BLE001
            r = {'error': str(e)}
        finally:
            pool.put(wt)
        r['modules'] = configs[label]
        r['worktree'] = wt.name
        with LOCK:
            results[label] = r
            out.write_text(json.dumps(results, indent=1))
        print(label, {k: v for k, v in r.items() if k not in ('modules', 'error')},
              'ERROR' if 'error' in r else '', flush=True)

    with concurrent.futures.ThreadPoolExecutor(a.jobs) as ex:
        list(ex.map(job, todo))


if __name__ == '__main__':
    main()

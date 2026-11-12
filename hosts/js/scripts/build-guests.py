"""Builds the mm-wasm guests the JS host is measured with, as `cargo xtask size` ships them.

    python3.14 -I hosts/js/scripts/build-guests.py [OUT_DIR]

- `default`: the shipped guest (`cargo xtask size`, which also checks the budgets),
  copied from target/wasm32-unknown-unknown/shipped/mm_wasm_guest.wasm.
- `bundle`: the shipped bundle guest from the same `cargo xtask size`
  (target/mm-wasm-bundle/.../mm_wasm_guest_bundle.wasm): its prelude and stdlib
  bytecode in custom sections, served through `mm_host.bundle_read`.
- `core35` and `nostdlib`: the same build with the facade's `default` feature list
  replaced by docs/sdk.md's code-mode list (34 features; "core35" in
  docs/research/stdlib-profile.md) or by nothing. Each builds in its own git
  worktree of HEAD at /home/user/workspace/wtc5 or wtns, the checkout's path
  length, as bench/evidence/stdlib_profile/removals.py does, so nothing
  path-dependent moves; only the worktree's crates/micro-monty/Cargo.toml changes.

OUT_DIR (default hosts/js/guests/, ignored by git) receives `<label>.wasm` and
`sizes.json` (bytes, gzip -9 and brotli bytes when `brotli` is installed).
"""
import gzip
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
UU = 'wasm32-unknown-unknown'
WASM_FEATURES = ['--enable-bulk-memory', '--enable-bulk-memory-opt',
                 '--enable-call-indirect-overlong', '--enable-multivalue',
                 '--enable-mutable-globals', '--enable-nontrapping-float-to-int',
                 '--enable-reference-types', '--enable-sign-ext']
WASM_OPT = os.environ.get('WASM_OPT') or str(Path.home() / '.local/bin/wasm-opt')

# docs/sdk.md, "Smaller module sets for code-mode agents", the code-mode list.
CORE35 = ['json', 're', 'collections', 'itertools', 'functools', 'datetime', 'random',
          'asyncio', 'typing', 'base64', 'hashlib', 'operator', 'heapq', 'bisect',
          'textwrap', 'abc', 'binascii', 'time', 'os', 'pathlib', 'fnmatch', 'csv', 'io',
          'urllib', 'struct', 'html', 'warnings', 'decimal', 'numbers', 'contextvars',
          'fractions', 'statistics', 'hmac', 'zlib']
VARIANTS = {'core35': ('wtc5', CORE35), 'nostdlib': ('wtns', [])}


def shipped_env():
    env = dict(os.environ, RUSTC_BOOTSTRAP='1',
               RUSTFLAGS='-Zunstable-options -Cpanic=immediate-abort -Cforce-unwind-tables=no --cfg mm_shipped')
    env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    env.pop('CARGO_TARGET_DIR', None)
    return env


def variant(name, wt_name, mods, out):
    wt = Path('/home/user/workspace') / wt_name
    assert len(str(wt)) == len(str(ROOT)), (wt, ROOT)
    if not wt.exists():
        subprocess.run(['git', 'worktree', 'add', '--detach', str(wt), 'HEAD'], cwd=ROOT,
                       check=True, capture_output=True)
    else:
        subprocess.run(['git', 'checkout', '--detach', '--force', 'HEAD'], cwd=wt, check=True,
                       capture_output=True)
        head = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, check=True,
                              capture_output=True, text=True).stdout.strip()
        subprocess.run(['git', 'checkout', '--detach', '--force', head], cwd=wt, check=True,
                       capture_output=True)
    toml = wt / 'crates/micro-monty/Cargo.toml'
    body = ''.join(f'    "mod-{m}",\n' for m in mods)
    text, n = re.subn(r'^default = \[\n.*?^\]\n', f'default = [\n{body}]\n',
                      (ROOT / 'crates/micro-monty/Cargo.toml').read_text(), count=1,
                      flags=re.S | re.M)
    assert n == 1
    toml.write_text(text)
    subprocess.run(['cargo', 'build', '--release', '--locked', '-Zbuild-std=std,panic_abort',
                    '--target', UU, '--package', 'mm-wasm-guest'], cwd=wt, env=shipped_env(),
                   check=True)
    subprocess.run([WASM_OPT, '-Oz', *WASM_FEATURES,
                    str(wt / 'target' / UU / 'release/mm_wasm_guest.wasm'), '-o',
                    str(out / f'{name}.wasm')], check=True)


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'hosts/js/guests'
    out.mkdir(parents=True, exist_ok=True)
    subprocess.run(['cargo', 'xtask', 'size'], cwd=ROOT, check=True)
    shutil.copy(ROOT / 'target' / UU / 'shipped/mm_wasm_guest.wasm',
                out / 'default.wasm')
    shutil.copy(ROOT / 'target/mm-wasm-bundle' / UU / 'shipped/mm_wasm_guest_bundle.wasm',
                out / 'bundle.wasm')
    for name, (wt, mods) in VARIANTS.items():
        variant(name, wt, mods, out)
    sizes = {}
    for name in ['default', 'bundle', *VARIANTS]:
        data = (out / f'{name}.wasm').read_bytes()
        sizes[name] = {'bytes': len(data), 'gzip9': len(gzip.compress(data, 9))}
        if shutil.which('brotli'):
            b = subprocess.run(['brotli', '-c', '-q', '11', str(out / f'{name}.wasm')],
                               check=True, capture_output=True).stdout
            sizes[name]['brotli11'] = len(b)
    (out / 'sizes.json').write_text(json.dumps(sizes, indent=1) + '\n')
    print(json.dumps(sizes, indent=1))


if __name__ == '__main__':
    main()

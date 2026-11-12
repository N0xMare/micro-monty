"""Stdlib usage per module feature: real plak cells and synthetic solutions.

    python3.14 -I bench/evidence/stdlib_profile/census.py OUT.json [--profile a,b,...]...

Real: every `start_cell` frame of conformance/codemode/real/traces.tar.xz
(2,841 cells, 160 sessions). A cell needs the modules its source imports and
the modules of the bootstrap plak sends with it (`prelude.bootstrap`; the
py-sdk bootstrap imports `asyncio` and `collections`), which the worker
runs for the cell's session. Synthetic: every solution under
conformance/codemode/solutions/ except the probes (`p/`): 1,782.

Imports come from the AST (top-level await allowed), at any depth, as
CPython 3.14 parses them; a source that does not parse imports nothing (it
fails before any import runs). Each import maps to the `mod-*` feature that
provides it (`os.path` to os, `html.parser` to html, `token` to tokenize);
`sys`, `cmath` and `_mm` are always built. A unit *uses* a module if it
imports it directly or imports a module whose feature closure contains it.

With `--profile`, also counts the units that import a module outside that
module set (its feature closure), by module.
"""
import argparse
import ast
import collections
import json
import sys
import tarfile
import warnings
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import features  # noqa: E402

ROOT = features.ROOT
ARCHIVE = ROOT / 'conformance/codemode/real/traces.tar.xz'
SOLUTIONS = ROOT / 'conformance/codemode/solutions'
ALWAYS = {'sys', 'cmath', '_mm', '__future__'}
SPECIAL = {'html.entities': 'html-entities', 'token': 'tokenize', '_markupbase': 'html'}
# Submodules that need a feature besides their package's.
ALSO = {'collections.abc': 'abc'}
FLAGS = ast.PyCF_ONLY_AST | ast.PyCF_ALLOW_TOP_LEVEL_AWAIT


def imports(source):
    """The imported module names, or None when the source does not parse."""
    try:
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            tree = compile(source, '<cell>', 'exec', FLAGS)
    except (SyntaxError, ValueError):
        return None
    out = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            out |= {a.name for a in node.names}
        elif isinstance(node, ast.ImportFrom) and node.module and not node.level:
            out.add(node.module)
    return out


def feature(module):
    """The feature providing `module`, '' if always built, else the name."""
    if module in ALWAYS:
        return ''
    if module in SPECIAL:
        return SPECIAL[module]
    top = module.split('.')[0]
    return top


def real_units():
    """(id, source imports, bootstrap imports, composition) per recorded cell."""
    with tarfile.open(ARCHIVE) as tar:
        for member in sorted(tar.getmembers(), key=lambda m: m.name):
            parts = member.name.split('/')
            if not member.isfile() or not parts[-1].startswith('trace-'):
                continue
            sid = '/'.join(parts[1:4])
            for line in tar.extractfile(member).read().decode().splitlines():
                frame = json.loads(line).get('f') or {}
                if frame.get('frame') != 'start_cell':
                    continue
                boot = ((frame.get('prelude') or {}).get('bootstrap') or {}).get('source', '')
                yield (f'{sid}/{frame["cell"]}', imports(frame['source']),
                       imports(boot) or set(), parts[2])


def synthetic_units():
    for path in sorted(SOLUTIONS.glob('*/*.py')):
        if path.parent.name == 'p':
            continue
        yield f'{path.parent.name}/{path.stem}', imports(path.read_text()), set(), path.parent.name


def tally(units, profiles):
    n = unparsed = 0
    direct = collections.Counter()
    used = collections.Counter()
    via_boot = collections.Counter()
    other = collections.Counter()
    fails = {name: collections.Counter() for name in profiles}
    fail_units = {name: [] for name in profiles}
    any_import = 0
    for uid, src, boot, _ in units:
        n += 1
        if src is None:
            unparsed += 1
            src = set()
        any_import += bool(src)
        feats = {feature(m) for m in src | boot} | {ALSO[m] for m in src | boot if m in ALSO}
        feats -= {''}
        default = {f for f in feats if f in features.DEPS and f in features.DEFAULT}
        for f in feats - set(default):
            other[f] += 1
        for f in ({feature(m) for m in src} | {ALSO[m] for m in src if m in ALSO}) & default:
            direct[f] += 1
        need = features.closure(default)
        for f in need:
            used[f] += 1
        for f in features.closure({feature(m) for m in boot} - {''}) - features.closure(
                {feature(m) for m in src} & default):
            via_boot[f] += 1
        for name, keep in profiles.items():
            missing = need - keep
            for f in missing:
                fails[name][f] += 1
            if missing:
                fail_units[name].append([uid, sorted(missing)])
    return {
        'units': n, 'unparsed': unparsed, 'with_any_import_in_source': any_import,
        'direct': dict(direct.most_common()), 'used': dict(used.most_common()),
        'only_through_bootstrap': dict(via_boot.most_common()),
        'not_default': dict(other.most_common()),
        'profiles': {name: {'failing_units': len(fail_units[name]),
                            'by_module': dict(fails[name].most_common()),
                            'units': fail_units[name]} for name in profiles},
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('out')
    ap.add_argument('--profile', action='append', default=[],
                    help='NAME=mod,mod,... (closed over features)')
    a = ap.parse_args()
    profiles = {}
    for spec in a.profile:
        name, mods = spec.split('=', 1)
        profiles[name] = features.closure([m for m in mods.split(',') if m])
    report = {'real': tally(real_units(), profiles),
              'synthetic': tally(synthetic_units(), profiles),
              'profiles': {k: sorted(v) for k, v in profiles.items()}}
    Path(a.out).write_text(json.dumps(report, indent=1))
    for corpus in ('real', 'synthetic'):
        r = report[corpus]
        print(f'{corpus}: {r["units"]} units, {r["unparsed"]} do not parse, '
              f'{r["with_any_import_in_source"]} import in their source')
        print('  used:', r['used'])
        print('  direct:', r['direct'])
        print('  only through the bootstrap:', r['only_through_bootstrap'])
        print('  not in the default set:', r['not_default'])
        for name, p in r['profiles'].items():
            print(f'  profile {name}: {p["failing_units"]} fail, {p["by_module"]}')


if __name__ == '__main__':
    main()

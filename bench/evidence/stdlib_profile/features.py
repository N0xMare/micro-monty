"""The module feature graph of micro-monty's default build, from the manifests.

    python3.14 -I bench/evidence/stdlib_profile/features.py   # prints the removal groups

`DEFAULT` is the facade's `default` list (crates/micro-monty/Cargo.toml),
`DEPS[m]` the `mod-*` features `mod-m` turns on in mm-stdlib (its own
list, not transitive). `closure(mods)` is every module a build listing
`mods` contains; `group(m)` is `m` plus every default module whose closure
holds `m`: the set the facade must drop for `m` to leave the build.

`UNDECLARED` are imports a module's Python source makes at import time
(top level, unguarded) that its feature does not declare, found by
`python3.14 -I features.py --check`; `closure` and `group` follow them too,
so a measured set never holds a module that fails at import. Imports made
only inside functions are not followed: `collections.abc` (a submodule
`mod-collections` builds, which `ChainMap`/`UserDict` views and typing's
generic subclassing import) needs `abc`, `time.strptime` needs datetime,
and `--check` lists the rest. `mod-math`
gates nothing: `math` is an engine module, always built (mm-engine
`modules::BUNDLED`).
"""
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def _load(root=ROOT):
    facade = tomllib.loads((root / 'crates/micro-monty/Cargo.toml').read_text())
    stdlib = tomllib.loads((root / 'crates/mm-stdlib/Cargo.toml').read_text())
    default = [f.removeprefix('mod-') for f in facade['features']['default']]
    deps = {}
    for name, items in stdlib['features'].items():
        if name.startswith('mod-'):
            deps[name.removeprefix('mod-')] = [i.removeprefix('mod-') for i in items
                                               if i.startswith('mod-')]
    return default, deps


DEFAULT, DEPS = _load()
UNDECLARED = {
    'datetime': ['operator'],
    'functools': ['collections'],
    'hashlib': ['operator'],
    'os': ['operator'],
    'random': ['itertools', 'operator'],
    'time': ['operator'],
    'typing': ['collections', 'functools'],
}


def closure(mods, declared_only=False):
    seen, todo = set(), list(mods)
    while todo:
        m = todo.pop()
        if m not in seen:
            seen.add(m)
            todo += DEPS[m] + ([] if declared_only else UNDECLARED.get(m, []))
    return seen


def group(m, declared_only=False):
    return {m} | {d for d in DEFAULT if m in closure([d], declared_only)}


def check():
    """Imports of each bundled module outside its feature's declared closure."""
    import ast
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from census import feature
    base = ROOT / 'crates/mm-stdlib/python'
    for path in sorted(base.rglob('*.py')):
        name = '.'.join(p for p in path.relative_to(base).with_suffix('').parts
                        if p != '__init__')
        own = feature(name)
        if own not in DEPS:
            continue
        found = {'import time': set(), 'in functions': set()}

        def visit(node, lazy):
            for child in ast.iter_child_nodes(node):
                inner = lazy or isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
                if isinstance(child, ast.Import):
                    mods = {a.name for a in child.names}
                elif isinstance(child, ast.ImportFrom) and child.module and not child.level:
                    mods = {child.module}
                else:
                    mods = set()
                found['in functions' if inner else 'import time'].update(mods)
                visit(child, inner)

        visit(ast.parse(path.read_text()), False)
        have = closure([own], declared_only=True)
        for when, mods in found.items():
            missing = sorted({feature(m) for m in mods} - {'', own, 'math'} - have)
            if missing:
                print(f'{name}: {when}: {", ".join(missing)}')


def without(mods):
    """The default list minus `mods`, as a facade `default` list."""
    return [m for m in DEFAULT if m not in mods]


if __name__ == '__main__':
    if sys.argv[1:] == ['--check']:
        check()
        sys.exit()
    for m in DEFAULT:
        g = group(m)
        print(f'{m}: {"alone" if g == {m} else ", ".join(sorted(g - {m}))}')

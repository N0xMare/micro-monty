"""The tables of docs/research/stdlib-profile.md, from the raw results.

    python3.14 -I bench/evidence/stdlib_profile/report.py [RESULTS_DIR]

Reads `removals.json` (removals.py), `usage.json` (census.py with every
profile of `profiles.json`) and `behaviour.json` (behaviour.py) and prints
Markdown.
"""
import collections
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import features  # noqa: E402

D = Path(sys.argv[1] if len(sys.argv) > 1 else
         features.ROOT / 'bench/evidence/results/2026-11-08-stdlib-profile')
BUILDS = ('hello.wasm', 'native hello', 'guest')


def n(x):
    return f'{x:,}'


def main():
    rem = json.loads((D / 'removals.json').read_text())
    use = json.loads((D / 'usage.json').read_text())
    beh = json.loads((D / 'behaviour.json').read_text())
    base = rem['baseline']
    R, S = use['real'], use['synthetic']
    print(f"Baseline: {', '.join(f'{b} {n(base[b])}' for b in BUILDS)}; "
          f"{R['units']} real cells, {S['units']} synthetic solutions.\n")

    rows = []
    for m in features.DEFAULT:
        v = rem[f'-{m}']
        saved = [base[b] - v[b] for b in BUILDS]
        r, s = R['used'].get(m, 0), S['used'].get(m, 0)
        deps = sorted(features.group(m) - {m})
        per = saved[0] / (r + s) if r + s else float('inf')
        rows.append((per, saved[0], m, saved, r, s, deps))
    rows.sort(key=lambda x: (-x[0], -x[1]))
    print('| module | hello.wasm | native hello | guest | real uses | synthetic uses '
          '| removed with it (imports it) | B/use |')
    print('|---|---:|---:|---:|---:|---:|---|---:|')
    for per, _, m, saved, r, s, deps in rows:
        pr = f'{r} ({r / R["units"]:.1%})' if r else '0'
        ps = f'{s} ({s / S["units"]:.1%})' if s else '0'
        perc = '∞' if per == float('inf') else n(round(per))
        print(f'| {m} | −{n(saved[0])} | −{n(saved[1])} | −{n(saved[2])} | {pr} | {ps} '
              f'| {", ".join(deps) or "alone"} | {perc} |')

    print('\n| set | modules | hello.wasm | native hello | guest | real changed | real '
          'completed → failed | synthetic changed | synthetic pass → fail |')
    print('|---|---:|---:|---:|---:|---:|---:|---:|---:|')
    order = ['baseline'] + [k for k in beh if k != 'baseline']
    for k in order:
        b, size = beh[k], rem.get(k)
        cells = (f"{n(size['hello.wasm'])} (−{n(base['hello.wasm'] - size['hello.wasm'])}) | "
                 f"{n(size['native hello'])} (−{n(base['native hello'] - size['native hello'])}) | "
                 f"{n(size['guest'])} (−{n(base['guest'] - size['guest'])})") if size else '| |'
        print(f"| {k} | {len(b['modules'])} | {cells} | {b['real']['changed']} | "
              f"{b['real']['completed_to_failed']} | {b['synthetic']['changed']} | "
              f"{b['synthetic']['pass_to_fail']} |")
    for k in ('none', 'struct+operator'):
        size = rem[k]
        print(f"| {k} (size only) | {len(size['modules'])} | "
              + ' | '.join(f"{n(size[b])} (−{n(base[b] - size[b])})" for b in BUILDS)
              + ' | | | | |')

    print('\nFailures by missing module (dynamic):\n')
    for k in order[1:]:
        b = beh[k]
        real = collections.Counter()
        for u in b['real']['changed_units']:
            mods = re.findall(r"No module named '([^']+)'", u['message']) or ['(other)']
            real[(mods[0], u['before'], u['after'])] += 1
        syn = collections.Counter()
        for u in b['synthetic']['changed_units']:
            mods = re.findall(r'import (\S+)', u.get('cause') or '') or [u.get('cause')]
            syn[(mods[0], u['before'], u['after'])] += 1
        fmt = lambda c: '; '.join(f'{m} {v} ({a}→{z})' for (m, a, z), v in c.most_common())  # noqa: E731
        print(f'- {k}: real: {fmt(real) or "none"}. synthetic: {fmt(syn) or "none"}.')

    print('\nStatic census (units importing a module outside the set):\n')
    for name, p in R['profiles'].items():
        q = S['profiles'][name]
        print(f"- {name}: real {p['failing_units']} {p['by_module']}; "
              f"synthetic {q['failing_units']} {q['by_module']}")


if __name__ == '__main__':
    main()

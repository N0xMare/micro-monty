#!/usr/bin/env python3
"""Instructions per operation (callgrind Ir), per engine.

    python3 bench/opcost.py [--engine NAME=CMD ...] [--only a,b] [-n N]

Each case is a loop body run N and 2N times inside a function; the
difference over N is the cost of one iteration, startup excluded. The
`loop` case (an empty body) is subtracted from the others, so a row is the
body's own cost. Engines as in `callgrind.py` (defaults: CPython 3.14 and
`target/release/mm run`). Prints a Markdown table of Ir per operation.
"""

import argparse
import concurrent.futures
import os
import re
import shlex
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# name: (setup at module level, setup in the function, loop body)
CASES = {
    "loop": ("", "", "pass"),
    "add": ("", "a = 1", "b = a + i"),
    "global load": ("G = 1", "", "b = G"),
    "list[i]": ("", "l = list(range(10))", "b = l[3]"),
    "dict[k]": ("", "d = {'a': 1, 'b': 2}", "b = d['a']"),
    "attr load": ("class C:\n    def __init__(s): s.x = 1", "o = C()", "b = o.x"),
    "attr store": ("class C:\n    def __init__(s): s.x = 1", "o = C()", "o.x = i"),
    "call f(x)": ("def f(x):\n    return x", "", "b = f(i)"),
    "method o.m(x)": ("class C:\n    def m(s, x): return x", "o = C()", "b = o.m(i)"),
    "new C(x)": ("class C:\n    def __init__(s, x): s.x = x", "", "b = C(i)"),
    "str method": ("", "s = 'a'", "b = s.isdigit()"),
    "len(x)": ("", "l = [1, 2]", "b = len(l)"),
    "lambda key call": ("", "k = lambda r: r", "b = k(i)"),
    "generator next": (
        "def g():\n    while True:\n        yield 1",
        "it = g()",
        "b = next(it)",
    ),
    "for over gen": ("def g(n):\n    for x in range(n):\n        yield x", "", None),
    "genexpr in sum": ("", "", None),
    "raise+except": (
        "",
        "",
        "try:\n            raise ValueError('x')\n        except ValueError:\n            pass",
    ),
    "raise in callee": (
        "def f():\n    raise ValueError('x')",
        "",
        "try:\n            f()\n        except ValueError:\n            pass",
    ),
    "try (no raise)": ("", "", "try:\n            b = i\n        except ValueError:\n            pass"),
    "KeyError caught": (
        "",
        "d = {}",
        "try:\n            b = d[i]\n        except KeyError:\n            pass",
    ),
    "f-string {i:06d}": ("", "", "b = f'u{i:06d}'"),
    "'%s:%d' % t": ("", "", "b = '%s:%d' % ('ab', i)"),
    "str(i)": ("", "", "b = str(i)"),
    "'|'.join(3)": ("", "", "b = '|'.join(['a', 'b', 'c'])"),
    "tuple build": ("", "", "b = (i, i, i)"),
    "list append": ("", "l = []", "l.append(i)"),
}


def program(name, n):
    mod, fn, body = CASES[name]
    if name == "for over gen":
        loop = f"    for x in g({n}):\n        pass"
    elif name == "genexpr in sum":
        loop = f"    b = sum(x for x in range({n}))"
    else:
        loop = f"    for i in range({n}):\n        {body}"
    init = f"    {fn}\n" if fn else ""
    return f"{mod}\ndef main():\n{init}{loop}\nmain()\n"


def ir(cmd, src, tmp):
    path = os.path.join(tmp, f"p{abs(hash((cmd, src)))}.py")
    with open(path, "w") as f:
        f.write(src)
    p = subprocess.run(
        ["valgrind", "--tool=callgrind", "--trace-children=yes",
         "--callgrind-out-file=/dev/null"] + shlex.split(cmd) + [path],
        capture_output=True, text=True, env=dict(os.environ, PYTHONHASHSEED="0"),
    )
    if p.returncode != 0:
        sys.exit(f"{cmd} failed on:\n{src}\n{p.stdout}{p.stderr[-2000:]}")
    return sum(int(m) for m in re.findall(r"Collected : (\d+)", p.stderr))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine", action="append", default=[])
    ap.add_argument("--only")
    ap.add_argument("-n", type=int, default=20000)
    a = ap.parse_args()
    engines = [e.split("=", 1) for e in a.engine] or [
        ["cpython", os.environ.get("PYTHON", "python3.14")],
        ["mm", os.path.join(ROOT, "target/release/mm") + " run"],
    ]
    names = list(CASES) if not a.only else ["loop"] + a.only.split(",")
    tmp = tempfile.mkdtemp(prefix="opcost-")
    jobs = [(e, c, k, m) for k in names for e, c in engines for m in (1, 2)]
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count() or 4) as ex:
        got = dict(zip(
            [(e, k, m) for e, _, k, m in jobs],
            ex.map(lambda j: ir(j[1], program(j[2], a.n * j[3]), tmp), jobs),
        ))
    subprocess.run(["rm", "-rf", tmp])
    per = {(e, k): (got[e, k, 2] - got[e, k, 1]) / a.n for e, _ in engines for k in names}
    en = [e for e, _ in engines]
    print("| Operation | " + " | ".join(f"{e} Ir" for e in en)
          + "".join(f" | {e} / {en[0]}" for e in en[1:]) + " |")
    print("|---" + "|---:" * (2 * len(en) - 1) + "|")
    for k in names:
        base = 0 if k in ("loop", "for over gen", "genexpr in sum") else 1
        v = [per[e, k] - base * per[e, "loop"] for e in en]
        print(f"| {k} | " + " | ".join(f"{x:,.0f}" for x in v)
              + "".join(f" | {x / v[0]:.2f}" if v[0] > 0 else " | -" for x in v[1:]) + " |")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3.14
"""Run selected CPython Lib/test modules on CPython and on micro-monty.

Every unit (see modules.txt) is turned into one self-contained file: the
shim (shim/mmunittest.py), helper modules from Lib/test inlined as
functions, then the test module with uniform source transforms (README.md,
"Transforms"). CPython discovers the test methods; the same file then runs
on CPython and on mm, one result line per method.

Usage:
  python3.14 -I harness.py --lib-test DIR --mm MM [--jobs 4] [--out RAW] [--only UNIT...]

Generated files and per-process output go to $MM_CPYTHON_TESTS_WORK
(default /tmp/mm-cpython-tests)/work/<unit>/; per-unit results to RAW
(default .../raw)/units/<unit>.json. report.py turns RAW into results/.
"""

import argparse
import ast
import concurrent.futures
import hashlib
import io
import json
import os
import re
import resource
import subprocess
import sys
import time
import tokenize

HERE = os.path.dirname(os.path.abspath(__file__))
SHIM_PATH = os.path.join(HERE, "shim", "mmunittest.py")
SHIM_FIRST_LINE = "import sys as _mm_sys"
COMPILE_CHECK_LINE = SHIM_FIRST_LINE + '; _mm_sys.stderr.write("@@MMCOMPILED\\n"); raise SystemExit(0)'

ATTEMPT_TIMEOUT = 120.0   # one engine process
MODULE_BUDGET = 600.0     # all attempts of one unit on one engine
MAX_STUBS = 300
WORK_ROOT = os.environ.get("MM_CPYTHON_TESTS_WORK", "/tmp/mm-cpython-tests")
# Pure-Python test.support submodules inlined like other Lib/test helpers.
INLINED_SUPPORT = ("support.testcase", "support.numbers")
MEM_LIMIT = 4 << 30


# ------------------------------------------------------------ transforms

def protected_lines(src):
    """Lines that start inside a multi-line token (string, f-string part):
    re-indenting them would change the string's value."""
    prot = set()
    try:
        toks = list(tokenize.generate_tokens(io.StringIO(src).readline))
    except (tokenize.TokenError, SyntaxError):
        return prot
    for t in toks:
        if t.end[0] > t.start[0] and t.type not in (tokenize.NL, tokenize.NEWLINE):
            for ln in range(t.start[0] + 1, t.end[0] + 1):
                prot.add(ln)
    return prot


def strip_comments(src, file_path=None):
    """Source lines with comment tokens removed (keeps line numbers; keeps
    the generated file small: mm refuses sources over 256 KiB). With
    file_path, every `__file__` name token becomes that path's literal."""
    lines = src.splitlines()
    try:
        toks = list(tokenize.generate_tokens(io.StringIO(src).readline))
    except (tokenize.TokenError, SyntaxError):
        return lines
    for t in reversed(toks):
        r, c = t.start
        if t.type == tokenize.COMMENT:
            lines[r - 1] = lines[r - 1][:c].rstrip()
        elif t.type == tokenize.NAME and t.string == "__file__" and file_path is not None:
            lines[r - 1] = lines[r - 1][:c] + repr(file_path) + lines[r - 1][t.end[1]:]
    return lines


def stmt_start(node):
    start = node.lineno
    for d in getattr(node, "decorator_list", ()):
        start = min(start, d.lineno)
    return start


def bound_names(body):
    """Names a module body binds at its top level."""
    names = []

    def add(n):
        if n not in names and not n.startswith("_mm"):
            names.append(n)

    def targets(t):
        for n in ast.walk(t):
            if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Store):
                add(n.id)

    def visit(stmts):
        for s in stmts:
            if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                add(s.name)
            elif isinstance(s, (ast.Import, ast.ImportFrom)):
                for a in s.names:
                    if a.name == "*":
                        continue
                    add(a.asname or a.name.split(".")[0])
            elif isinstance(s, (ast.Assign, ast.AnnAssign, ast.AugAssign)):
                for t in (s.targets if isinstance(s, ast.Assign) else [s.target]):
                    targets(t)
            elif isinstance(s, (ast.For, ast.AsyncFor)):
                targets(s.target)
                visit(s.body)
                visit(s.orelse)
            elif isinstance(s, (ast.If, ast.While)):
                visit(s.body)
                visit(s.orelse)
            elif isinstance(s, (ast.With, ast.AsyncWith)):
                for it in s.items:
                    if it.optional_vars is not None:
                        targets(it.optional_vars)
                visit(s.body)
            elif isinstance(s, (ast.Try, ast.TryStar)):
                visit(s.body)
                for h in s.handlers:
                    if h.name:
                        add(h.name)
                    visit(h.body)
                visit(s.orelse)
                visit(s.finalbody)
            elif isinstance(s, ast.Expr):
                for n in ast.walk(s):
                    if isinstance(n, ast.NamedExpr):
                        add(n.target.id)
    visit(body)
    return names


def wildcard_names(mod):
    """Names `from mod import *` binds on the oracle (CPython)."""
    out = subprocess.run([sys.executable, "-I", "-c",
                          "import importlib,sys; m=importlib.import_module(sys.argv[1]); "
                          "print(' '.join(getattr(m, '__all__', None) or "
                          "[n for n in vars(m) if not n.startswith('_')]))", mod],
                         capture_output=True, text=True, check=True)
    return out.stdout.split()


class Builder:
    """Builds one generated file. Lines are (text, origin) pairs, origin a
    'file:line' string or '' for generated lines."""

    def __init__(self, lib_test):
        self.lib_test = lib_test
        self.helpers = {}        # dotted name -> function name
        self.helper_order = []   # (dotted, lines)
        self.helper_stack = []
        self.known_modules = []
        self.stub_modules = set()
        self.notes = []

    # Lib/test module resolution
    def resolve(self, dotted):
        """dotted is relative to Lib/test (no 'test.' prefix)."""
        if not dotted or (dotted == "support" or dotted.startswith("support.")) and \
                dotted not in INLINED_SUPPORT:
            return None
        p = os.path.join(self.lib_test, *dotted.split("."))
        if os.path.isfile(p + ".py"):
            return p + ".py"
        if os.path.isfile(os.path.join(p, "__init__.py")):
            return os.path.join(p, "__init__.py")
        return None

    def helper_expr(self, dotted):
        """Expression for the namespace of Lib/test module `dotted`."""
        if dotted == "support":
            return "_mm_support"
        if dotted.startswith("support.") and dotted not in INLINED_SUPPORT:
            return "_mm_support." + dotted[len("support."):]
        path = self.resolve(dotted)
        if path is None:
            return "_mm_missing(%r)" % ("test." + dotted)
        if dotted not in self.helpers:
            if dotted in self.helper_stack:
                return "_mm_missing(%r)" % ("test." + dotted + " (import cycle)")
            fn = "_mm_mod_" + re.sub(r"\W", "_", dotted)
            self.helper_stack.append(dotted)
            with open(path, encoding="utf-8") as f:
                src = f.read()
            pkg = dotted if path.endswith("__init__.py") else dotted.rpartition(".")[0]
            body = self.transform(src, os.path.relpath(path, self.lib_test), pkg, helper=True)
            self.helper_stack.pop()
            self.helpers[dotted] = fn
            tree = ast.parse(src)
            names = bound_names(tree.body)
            lines = [("def _mm_modfn_%s():" % fn[len("_mm_mod_"):], "")]
            for text, origin in body:
                lines.append((text if text == "" else " " + text, origin) if not origin.endswith("!prot") else (text, origin[:-5]))
            lines.append((" _mm_ns = _MMNamespace(%r)" % ("test." + dotted), ""))
            for n in names:
                lines.append((" try:", ""))
                lines.append(("  _mm_ns.%s = %s" % (n, n), ""))
                lines.append((" except NameError:", ""))
                lines.append(("  pass", ""))
            lines.append((" return _mm_ns", ""))
            lines.append(("try:", ""))
            lines.append(("    %s = _mm_modfn_%s()" % (fn, fn[len("_mm_mod_"):]), ""))
            lines.append(("except BaseException as _mm_e:", ""))
            lines.append(("    _mm_toplevel_error(0, _mm_e)", ""))
            lines.append(("    %s = _mm_missing(%r)" % (fn, "test." + dotted + " (failed)"), ""))
            self.helper_order.append((dotted, lines))
        return self.helpers[dotted]

    def test_pkg_chain(self, parts):
        """Statements making `test.a.b` reachable through _mm_test_pkg."""
        out = []
        prefix = "_mm_test_pkg"
        acc = []
        for i, p in enumerate(parts):
            acc.append(p)
            dotted = ".".join(acc)
            if i == len(parts) - 1:
                out.append("%s.%s = %s" % (prefix, p, self.helper_expr(dotted)))
            elif dotted == "support":
                out.append("%s.support = _mm_support" % prefix)
            else:
                out.append("%s.%s = _MMNamespace(%r)" % (prefix, p, "test." + dotted))
            prefix = prefix + "." + p
        return out

    def import_lines(self, node, pkg, top):
        """Replacement statements for an import, or None to keep it."""
        out = []
        if isinstance(node, ast.Import):
            for a in node.names:
                name = a.name
                first = name.split(".")[0]
                if first == "unittest":
                    if a.asname:
                        out.append("%s = %s" % (a.asname, "_mm_unittest" + name[len("unittest"):]))
                    else:
                        out.append("unittest = _mm_unittest")
                elif first == "test":
                    parts = name.split(".")[1:]
                    if a.asname:
                        out.append("%s = %s" % (a.asname, self.helper_expr(".".join(parts))))
                    else:
                        out.extend(self.test_pkg_chain(parts))
                        out.append("test = _mm_test_pkg")
                elif top:
                    bind = a.asname or first
                    out.append("try:")
                    out.append("    import %s%s" % (name, " as " + a.asname if a.asname else ""))
                    out.append("except ImportError as _mm_ie:")
                    out.append("    %s = _mm_missing_module(%r, str(_mm_ie))" % (bind, name))
                    self.stub_modules.add(name)
                else:
                    return None
            return out
        # ImportFrom
        mod = node.module or ""
        if node.level:
            base = pkg
            for _ in range(node.level - 1):
                base = base.rpartition(".")[0]
            mod = (base + "." + mod).strip(".") if mod else base
            mod = "test." + mod if mod else "test"
        first = mod.split(".")[0]
        if any(a.name == "*" for a in node.names):
            if first in ("unittest", "test"):
                self.notes.append("wildcard import from %s left as is" % mod)
                return None
            if top and not self.helper_stack:
                return None
            # inside an inlined helper (a function body) `import *` is a
            # SyntaxError: expand it to the names CPython's module exports
            names = wildcard_names(mod)
            self.notes.append("wildcard import from %s expanded to %d names" % (mod, len(names)))
            out.append("try:")
            out.append("    from %s import (%s)" % (mod, ", ".join(names)))
            out.append("except ImportError as _mm_ie:")
            for n in names:
                out.append("    %s = _mm_missing_module(%r, str(_mm_ie))" % (n, mod + "." + n))
            return out
        if first == "unittest":
            src = "_mm_unittest" + ("" if mod == "unittest" else "." + mod.split(".", 1)[1].replace(".", "."))
            for a in node.names:
                out.append("%s = _mm_from(%s, %r)" % (a.asname or a.name, src, a.name))
            return out
        if first == "test":
            sub = mod[len("test."):] if mod != "test" else ""
            for a in node.names:
                full = (sub + "." + a.name).strip(".")
                bind = a.asname or a.name
                if full == "support" or full.startswith("support.") and sub == "support":
                    out.append("%s = _mm_from(_mm_support, %r)" % (bind, a.name) if sub == "support"
                               else "%s = _mm_support" % bind)
                elif sub.startswith("support.") and sub not in INLINED_SUPPORT:
                    out.append("%s = _mm_from(_mm_support.%s, %r)" % (bind, sub[len("support."):], a.name))
                elif self.resolve(full) is not None:
                    out.append("%s = %s" % (bind, self.helper_expr(full)))
                elif sub and self.resolve(sub) is not None:
                    out.append("%s = _mm_from(%s, %r)" % (bind, self.helper_expr(sub), a.name))
                else:
                    out.append("%s = _mm_missing(%r)" % (bind, "test." + full))
            return out
        if not top:
            return None
        for a in node.names:
            bind = a.asname or a.name
            out.append("try:")
            out.append("    from %s import %s%s" % (mod, a.name, " as " + a.asname if a.asname else ""))
            out.append("except ImportError as _mm_ie:")
            out.append("    %s = _mm_missing_module(%r, str(_mm_ie))" % (bind, mod + "." + a.name))
            self.stub_modules.add(mod + "." + a.name)
        return out

    def transform(self, src, relpath, pkg, helper=False):
        """Transform one module's source into (text, origin) lines. Origins
        of lines that must not be re-indented end with '!prot'."""
        tree = ast.parse(src)
        lines = strip_comments(src)
        prot = protected_lines(src)
        for n in ast.walk(tree):
            if isinstance(n, ast.Call):
                f = n.func
                fname = f.attr if isinstance(f, ast.Attribute) else getattr(f, "id", None)
                if fname in ("import_module", "import_fresh_module") and n.args and \
                        isinstance(n.args[0], ast.Constant) and isinstance(n.args[0].value, str):
                    m = n.args[0].value
                    if m not in self.known_modules and re.fullmatch(r"[A-Za-z_][\w.]*", m):
                        self.known_modules.append(m)
        # Nested unittest/test imports are rewritten in place (one line).
        replace = {}  # start line -> (end line, [text lines]) for nested imports
        top_ids = set(id(s) for s in tree.body)
        for n in ast.walk(tree):
            if isinstance(n, (ast.Import, ast.ImportFrom)) and id(n) not in top_ids:
                mod = n.module if isinstance(n, ast.ImportFrom) else n.names[0].name
                mod = mod or ""
                if not (mod.split(".")[0] in ("unittest", "test") or (isinstance(n, ast.ImportFrom) and n.level)):
                    continue
                line = lines[n.lineno - 1]
                if line[:n.col_offset].strip() or n.end_lineno is None:
                    continue
                rep = self.import_lines(n, pkg, top=False)
                if rep is None:
                    continue
                endl = lines[n.end_lineno - 1][n.end_col_offset:].strip()
                if endl and not endl.startswith("#"):
                    continue
                replace[n.lineno] = (n.end_lineno, line[:n.col_offset] + "; ".join(rep))

        def orig(i):
            return "%s:%d%s" % (relpath, i, "!prot" if i in prot else "")

        def raw(a, b):
            """Original lines a..b (1-based, inclusive) with nested import
            replacements applied."""
            res = []
            i = a
            while i <= b:
                if i in replace:
                    end, text = replace[i]
                    res.append((text, orig(i)))
                    for j in range(i + 1, end + 1):
                        res.append(("", orig(j)))
                    i = end + 1
                    continue
                res.append((lines[i - 1], orig(i)))
                i += 1
            return res

        out = []
        # group statements that share lines
        groups = []
        for s in tree.body:
            a, b = stmt_start(s), s.end_lineno
            if groups and a <= groups[-1][2]:
                groups[-1][0].append(s)
                groups[-1][2] = max(groups[-1][2], b)
            else:
                groups.append([[s], a, b])
        cursor = 1
        for stmts, a, b in groups:
            out.extend(raw(cursor, a - 1))   # comments / blank lines between
            cursor = b + 1
            s = stmts[0]
            if len(stmts) == 1 and isinstance(s, ast.ImportFrom) and s.module == "__future__":
                if helper:
                    self.notes.append("__future__ import in helper %s dropped" % relpath)
                    out.append(("pass", orig(a)))
                else:
                    self.future.append(lines[a - 1])
                    out.append(("pass", orig(a)))
                for j in range(a + 1, b + 1):
                    out.append(("", orig(j)))
                continue
            if len(stmts) == 1 and isinstance(s, ast.If) and isinstance(s.test, ast.Compare) and \
                    isinstance(s.test.left, ast.Name) and s.test.left.id == "__name__" and \
                    s.test.left.lineno == a:
                # `if __name__ == '__main__':` is false for an imported test
                # module; keep any else branch
                c0 = s.test.left.col_offset
                lines[a - 1] = lines[a - 1][:c0] + "'_mm_imported'" + lines[a - 1][c0 + len("__name__"):]
            if len(stmts) == 1 and isinstance(s, ast.ImportFrom) and any(n.name == "*" for n in s.names) \
                    and (s.module or "").split(".")[0] not in ("unittest", "test") and not s.level and not helper:
                out.extend(raw(a, b))
                continue
            if len(stmts) == 1 and isinstance(s, (ast.Import, ast.ImportFrom)):
                rep = self.import_lines(s, pkg, top=True)
                if rep is not None:
                    out.append((rep[0], orig(a)))
                    for t in rep[1:]:
                        out.append((t, ""))
                    for j in range(a + 1, b + 1):
                        out.append(("", orig(j)))
                    continue
                out.extend(raw(a, b))
                continue
            # wrap in try/except, re-indenting all but protected lines
            out.append(("try:", ""))
            for text, o in raw(a, b):
                if o.endswith("!prot") or text == "":
                    out.append((text, o))
                else:
                    out.append((" " + text, o))
            out.append(("except BaseException as _mm_e:", ""))
            out.append(("    _mm_toplevel_error(%r, _mm_e)" % ("%s:%d" % (relpath, a)), ""))
        out.extend(raw(cursor, len(lines)))
        return out

    def build(self, unit):
        """Returns (lines, info). lines is a list of (text, origin)."""
        self.future = []
        parts = unit.split(".")
        path = self.resolve(unit)
        if path is None:
            raise FileNotFoundError(unit)
        with open(path, encoding="utf-8") as f:
            src = f.read()
        pkg = unit if path.endswith("__init__.py") else unit.rpartition(".")[0]
        main = self.transform(src, os.path.relpath(path, self.lib_test), pkg)
        tree = ast.parse(src)
        top_names = bound_names(tree.body)
        with open(SHIM_PATH, encoding="utf-8") as f:
            shim = [t for t in strip_comments(f.read()) if t.strip()]
        assert shim.count(SHIM_FIRST_LINE) == 1
        lines = []
        for fl in self.future:
            lines.append((fl, "future"))
        for t in shim:
            lines.append((t, "shim"))
        lines.append(("_mm_test_pkg = _MMNamespace('test')", ""))
        lines.append(("_mm_test_pkg.support = _mm_support", ""))
        for m in self.known_modules:
            lines.append(("try:", ""))
            lines.append(("    import %s as _mm_tmp" % m, ""))
            lines.append(("    _mm_known_modules[%r] = _mm_tmp" % m, ""))
            lines.append(("except ImportError:", ""))
            lines.append(("    pass", ""))
        for _, hl in self.helper_order:
            lines.extend(hl)
        main_start = len(lines) + 1
        for text, o in main:
            lines.append((text, o[:-5] if o.endswith("!prot") else o))
        info = {
            "main_start": main_start,
            "has_setup_module": "setUpModule" in top_names,
            "has_teardown_module": "tearDownModule" in top_names,
            "helpers": [d for d, _ in self.helper_order],
            "stubbed_imports": sorted(self.stub_modules),
            "notes": self.notes,
        }
        return lines, info


def footer_discover(info):
    return ["_mm_discover(globals(), %d)" % info["main_start"]]


def footer_run(info, plan):
    out = ["_MM_PLAN = %r" % (plan,), "_MM_CLASSES = {"]
    for cname, _ in plan:
        out.append("    %r: (lambda: %s)," % (cname, cname))
    out.append("}")
    out.append("_mm_run(_MM_PLAN, _MM_CLASSES, %s, %s)" % (
        "(lambda: setUpModule())" if info["has_setup_module"] else "None",
        "(lambda: tearDownModule())" if info["has_teardown_module"] else "None"))
    return out


# ------------------------------------------------------------ processes

def limit_memory():
    resource.setrlimit(resource.RLIMIT_AS, (MEM_LIMIT, MEM_LIMIT))


def run_proc(cmd, timeout, workdir):
    t0 = time.monotonic()
    with open(os.path.join(workdir, "stdout"), "wb") as so, open(os.path.join(workdir, "stderr"), "wb") as se:
        p = subprocess.Popen(cmd, stdout=so, stderr=se, stdin=subprocess.DEVNULL,
                             cwd=workdir, preexec_fn=limit_memory)
        try:
            rc = p.wait(timeout=timeout)
            timed_out = False
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait()
            rc = None
            timed_out = True
    with open(os.path.join(workdir, "stderr"), "rb") as f:
        err = f.read().decode("utf-8", "replace")
    return rc, timed_out, err, time.monotonic() - t0


def unescape(s):
    out = []
    i = 0
    while i < len(s):
        c = s[i]
        if c == "\\" and i + 1 < len(s):
            n = s[i + 1]
            out.append({"t": "\t", "n": "\n", "r": "\r", "\\": "\\"}.get(n, n))
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def parse_lines(err):
    recs = []
    for ln in err.splitlines():
        if ln.startswith("@@MM"):
            recs.append([unescape(x) for x in ln.split("\t")])
    return recs


def non_protocol_tail(err, n=6):
    rest = [ln for ln in err.splitlines() if not ln.startswith("@@MM")]
    return "\n".join(rest[-n:])


def engine_cmd(engine, mm, path):
    if engine == "cpython":
        return [sys.executable, "-I", "-X", "utf8", path]
    return [mm, "run", path]


def run_engine(engine, mm, lines, info, plan, workdir, budget=MODULE_BUDGET):
    """Runs plan on one engine with crash/timeout restarts. Returns
    (results: id -> dict, toplevel errors, attempts)."""
    results = {}
    tops = []
    attempts = []
    order = [(c, m) for c, ms in plan for m in ms]
    deadline = time.monotonic() + budget
    while True:
        remaining = [(c, m) for c, m in order if c + "." + m not in results]
        if not remaining:
            break
        left = deadline - time.monotonic()
        if left <= 1:
            for c, m in remaining:
                results[c + "." + m] = {"status": "timeout", "type": "ModuleBudget",
                                        "msg": "module budget of %ds exhausted" % budget, "line": ""}
            break
        sub = []
        for c, m in remaining:
            if sub and sub[-1][0] == c:
                sub[-1][1].append(m)
            else:
                sub.append((c, [m]))
        path = os.path.join(workdir, "%s.py" % engine)
        with open(path, "w", encoding="utf-8") as f:
            f.write("\n".join([t for t, _ in lines] + footer_run(info, sub)) + "\n")
        rc, timed_out, err, dt = run_proc(engine_cmd(engine, mm, path), min(ATTEMPT_TIMEOUT, left), workdir)
        recs = parse_lines(err)
        attempts.append({"rc": rc, "timeout": timed_out, "seconds": round(dt, 2), "methods": len(remaining)})
        started = None
        done = False
        for r in recs:
            if r[0] == "@@MMSTART":
                started = r[1]
            elif r[0] == "@@MMT" and len(r) >= 6:
                results[r[1]] = {"status": r[2], "type": r[3], "line": r[4], "msg": r[5]}
                started = None
            elif r[0] == "@@MMTOP":
                if r not in tops:
                    tops.append(r[1:])
            elif r[0] == "@@MMDONE":
                done = True
        if done:
            for c, m in remaining:
                results.setdefault(c + "." + m, {"status": "error", "type": "NoResult",
                                                 "msg": "runner finished without a result", "line": ""})
            break
        tail = non_protocol_tail(err)
        if started is not None:
            results[started] = {"status": "timeout" if timed_out else "crash",
                                "type": "Timeout" if timed_out else "Crash",
                                "msg": ("killed after %ds" % int(dt)) if timed_out else "rc=%s: %s" % (rc, tail),
                                "line": ""}
            continue
        if any(r[0] == "@@MMT" for r in recs):
            # process died between methods (class setup/teardown): attribute
            # the rest to a process failure
            kind = "timeout" if timed_out else "crash"
        else:
            kind = "timeout" if timed_out else "load"
        for c, m in remaining:
            if c + "." + m in results:
                continue
            results[c + "." + m] = {"status": kind,
                                    "type": {"timeout": "Timeout", "crash": "Crash", "load": "ModuleLoad"}[kind],
                                    "msg": ("killed after %ds" % int(dt)) if timed_out else "rc=%s: %s" % (rc, tail),
                                    "line": ""}
        break
    for r in results.values():
        r["where"] = locate(lines, r.get("line", ""))
    return results, tops, attempts


def locate(lines, linestr):
    """Innermost traceback line that maps to test source (not the shim)."""
    best = ""
    for part in str(linestr).split(","):
        if not part.isdigit():
            continue
        n = int(part)
        if 1 <= n <= len(lines):
            o = lines[n - 1][1]
            if o and o not in ("shim", "future"):
                best = o
    return best


# ---------------------------------------------------------- mm source limit

MM_SOURCE_LIMIT = 256 * 1024
CHUNK_TARGET = MM_SOURCE_LIMIT - 16384


def file_size(lines, footer):
    return sum(len(t.encode("utf-8")) + 1 for t, _ in lines) + sum(len(t) + 1 for t in footer)


def make_chunks(lines, info, plan):
    """mm refuses sources over 256 KiB. When the generated file is larger,
    split the plan into chunks; each chunk's file blanks out the top-level
    test classes of other chunks. Only classes CPython discovered as test
    classes and that no other code names are removed."""
    if file_size(lines, footer_run(info, plan)) <= CHUNK_TARGET:
        return [(lines, plan)], None
    src = "\n".join(t for t, _ in lines) + "\n"
    tree = ast.parse(src)
    plan_names = set(c for c, _ in plan)
    spans = {}
    for s in tree.body:
        inner = s
        if isinstance(s, ast.Try) and len(s.handlers) == 1 and s.handlers[0].name == "_mm_e" and len(s.body) == 1:
            inner = s.body[0]
        if isinstance(inner, ast.ClassDef) and inner.name in plan_names and stmt_start(s) >= info["main_start"]:
            spans[inner.name] = (stmt_start(s), s.end_lineno, inner)
    refs = {}
    for n in ast.walk(tree):
        if isinstance(n, ast.Name) and n.id in spans:
            refs.setdefault(n.id, []).append(n.lineno)
    prunable = {}
    for name, (a, b, node) in spans.items():
        outside = [ln for ln in refs.get(name, []) if not a <= ln <= b]
        if not outside:
            prunable[name] = (a, b)
    def size_of(a, b):
        return sum(len(lines[j - 1][0].encode("utf-8")) + 1 for j in range(a, b + 1))
    base = file_size(lines, []) - sum(size_of(a, b) for a, b in prunable.values())
    chunks = []
    cur = []
    cur_size = base
    for c, ms in plan:
        if c not in prunable:
            continue
        sz = size_of(*prunable[c]) + sum(len(m) + 6 for m in ms) + 2 * len(c) + 40
        if cur and cur_size + sz > CHUNK_TARGET:
            chunks.append(cur)
            cur, cur_size = [], base
        cur.append(c)
        cur_size += sz
    if cur:
        chunks.append(cur)
    keep_always = [c for c, _ in plan if c not in prunable]
    if not chunks:
        chunks = [[]]
    chunks[0] = keep_always + chunks[0]
    out = []
    for names in chunks:
        sel = set(names)
        cl = list(lines)
        for name, (a, b) in prunable.items():
            if name in sel:
                continue
            for j in range(a, b + 1):
                cl[j - 1] = ("", cl[j - 1][1])
        out.append((cl, [(c, ms) for c, ms in plan if c in sel]))
    info_out = {"chunks": len(out), "unpruned_bytes": file_size(lines, footer_run(info, plan)),
                "not_prunable": keep_always,
                "chunk_bytes": [file_size(cl, footer_run(info, cp)) for cl, cp in out]}
    return out, info_out


# ---------------------------------------------------------- mm compile isolation

SYNTAX_RE = re.compile(r'^\s*File "[^"]*", line (\d+)')


def mm_compile_isolate(mm, lines, info, workdir):
    """Repeatedly compile the generated file on mm; while mm rejects it at
    compile time, replace the smallest enclosing unit (outermost def, else
    the enclosing class, else the top-level statement) with a stub that
    raises _MMExcluded. Returns (lines, stubs, fatal error or None)."""
    lines = list(lines)
    stubs = []
    seen_lines = set()
    for _ in range(MAX_STUBS):
        text = [COMPILE_CHECK_LINE if (t == SHIM_FIRST_LINE and o == "shim") else t for t, o in lines]
        path = os.path.join(workdir, "compile.py")
        with open(path, "w", encoding="utf-8") as f:
            f.write("\n".join(text + footer_discover(info)) + "\n")
        rc, timed_out, err, _ = run_proc([mm, "run", path], 60, workdir)
        if "@@MMCOMPILED" in err:
            return lines, stubs, None
        m = None
        for ln in err.splitlines():
            mm_ = SYNTAX_RE.match(ln)
            if mm_:
                m = int(mm_.group(1))
                break
        last = [ln for ln in err.splitlines() if ln.strip()]
        msg = last[-1].strip() if last else "rc=%s" % rc
        if timed_out:
            return lines, stubs, "compile timed out"
        if m is None or m < 1 or m > len(lines):
            return lines, stubs, "compile error without location: " + msg
        if lines[m - 1][1] in ("shim", "future"):
            return lines, stubs, "compile error in shim line %d: %s" % (m, msg)
        src = "\n".join(t for t, _ in lines) + "\n"
        tree = ast.parse(src)
        chain = []

        def descend(body):
            for s in body:
                if stmt_start(s) <= m <= s.end_lineno:
                    chain.append(s)
                    for field in ("body", "orelse", "finalbody"):
                        sub = getattr(s, field, None)
                        if isinstance(sub, list):
                            descend(sub)
                    for h in getattr(s, "handlers", None) or []:
                        descend(h.body)
                    for c in getattr(s, "cases", None) or []:
                        descend(c.body)
                    return
        if m in seen_lines:
            return lines, stubs, "compile error persists at line %d after stubbing: %s" % (m, msg)
        seen_lines.add(m)
        descend(tree.body)
        if not chain:
            return lines, stubs, "compile error outside any statement: line %d: %s" % (m, msg)

        def is_wrapper(s):
            return isinstance(s, ast.FunctionDef) and s.name.startswith("_mm_modfn_")

        def is_try_wrapper(s):
            return isinstance(s, ast.Try) and len(s.handlers) == 1 and \
                s.handlers[0].name == "_mm_e"

        target = None
        kind = None
        # the outermost method (a def directly in a class body), else the
        # outermost def
        for i, s in enumerate(chain):
            if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef)) and i > 0 and \
                    isinstance(chain[i - 1], ast.ClassDef):
                target, kind = s, "def"
                break
        if target is None:
            for s in chain:
                if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef)) and not is_wrapper(s):
                    target, kind = s, "def"
                    break
        if target is None:
            for s in chain:
                if isinstance(s, ast.ClassDef):
                    target, kind = s, "class"
                    break
        if target is None:
            for s in chain:
                if is_wrapper(s) or is_try_wrapper(s):
                    continue
                target, kind = s, "stmt"
                break
        if target is None:
            return lines, stubs, "cannot isolate compile error at line %d: %s" % (m, msg)
        a, b = stmt_start(target), target.end_lineno
        indent = re.match(r"\s*", lines[a - 1][0]).group(0)
        reason = "mm compile error: %s" % msg
        if kind == "def":
            in_deco = any(d.lineno <= m <= d.end_lineno for d in target.decorator_list)
            new = []
            if not in_deco:
                for d in target.decorator_list:
                    for j in range(d.lineno, d.end_lineno + 1):
                        new.append(j)
            body_start = target.lineno
            repl = {}
            for j in range(a, b + 1):
                repl[j] = ""
            for j in new:
                repl[j] = lines[j - 1][0]
            repl[body_start] = "%sdef %s(*_mm_a, **_mm_k): raise _mm_excluded(%r)" % (
                re.match(r"\s*", lines[body_start - 1][0]).group(0), target.name, reason)
        elif kind == "class":
            repl = {j: "" for j in range(a, b + 1)}
            repl[a] = "%s%s = _mm_excluded_class(%r)" % (indent, target.name, reason)
        else:
            repl = {j: "" for j in range(a, b + 1)}
            repl[a] = "%s_mm_toplevel_error(%r, _mm_excluded(%r))" % (indent, lines[a - 1][1], reason)
        for j, t in repl.items():
            lines[j - 1] = (t, lines[j - 1][1])
        name = getattr(target, "name", "")
        stubs.append({"kind": kind, "name": name if isinstance(name, str) else ast.unparse(name), "origin": lines[a - 1][1],
                      "line": a, "error": msg})
    return lines, stubs, "too many compile errors (> %d)" % MAX_STUBS


# ------------------------------------------------------------ one unit

def run_unit(unit, lib_test, mm, outdir):
    workdir = os.path.join(outdir, "work", unit)
    os.makedirs(workdir, exist_ok=True)
    rec = {"unit": unit}
    t0 = time.monotonic()
    # data files: link the non-.py entries of the unit's source directory
    # into the working directory, where the generated file (__file__) lives
    srcdir = os.path.join(lib_test, *unit.split(".")[:-1])
    for name in os.listdir(srcdir):
        if name.endswith(".py") or name == "__pycache__":
            continue
        dst = os.path.join(workdir, name)
        if not os.path.lexists(dst):
            os.symlink(os.path.join(srcdir, name), dst)
    try:
        b = Builder(lib_test)
        lines, info = b.build(unit)
    except Exception as e:
        rec["fatal"] = "transform failed: %s: %s" % (type(e).__name__, e)
        rec["methods"] = {}
        return rec
    rec["info"] = info
    # discovery on CPython
    path = os.path.join(workdir, "discover.py")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join([t for t, _ in lines] + footer_discover(info)) + "\n")
    rc, timed_out, err, _ = run_proc(engine_cmd("cpython", mm, path), ATTEMPT_TIMEOUT, workdir)
    plan = []
    for r in parse_lines(err):
        if r[0] == "@@MMCLASS" and len(r) >= 3 and r[2]:
            plan.append((r[1], r[2].split(",")))
    if not any(r[0] == "@@MMDONE" for r in parse_lines(err)):
        rec["fatal"] = "CPython discovery failed: rc=%s timeout=%s: %s" % (rc, timed_out, non_protocol_tail(err))
    rec["discover_toplevel_errors"] = [r[1:] for r in parse_lines(err) if r[0] == "@@MMTOP"]
    with open(os.path.join(workdir, "generated.py"), "w", encoding="utf-8") as f:
        f.write("\n".join([t for t, _ in lines] + footer_run(info, plan)) + "\n")
    cp, cp_tops, cp_att = run_engine("cpython", mm, lines, info, plan, workdir)
    rec["cpython_toplevel_errors"] = cp_tops
    rec["cpython_attempts"] = cp_att
    # mm: split into chunks if over mm's source limit, compile isolation,
    # then run each chunk
    chunks, rec["mm_chunking"] = make_chunks(lines, info, plan)
    rec["mm_stubs"] = []
    rec["mm_fatal"] = []
    rec["mm_toplevel_errors"] = []
    rec["mm_attempts"] = []
    mres = {}
    deadline = time.monotonic() + MODULE_BUDGET
    for k, (clines, cplan) in enumerate(chunks):
        mm_lines, stubs, fatal = mm_compile_isolate(mm, clines, info, workdir)
        rec["mm_stubs"].extend(stubs)
        if fatal is not None:
            rec["mm_fatal"].append(fatal)
            for c, ms in cplan:
                for m in ms:
                    mres[c + "." + m] = {"status": "load", "type": "CompileError", "msg": fatal,
                                         "line": "", "where": ""}
            continue
        with open(os.path.join(workdir, "generated-mm-%d.py" % k), "w", encoding="utf-8") as f:
            f.write("\n".join([t for t, _ in mm_lines] + footer_run(info, cplan)) + "\n")
        r, tops, att = run_engine("mm", mm, mm_lines, info, cplan, workdir,
                                  budget=max(deadline - time.monotonic(), 0))
        mres.update(r)
        for t in tops:
            if t not in rec["mm_toplevel_errors"]:
                rec["mm_toplevel_errors"].append(t)
        rec["mm_attempts"].extend(att)
    methods = {}
    for c, ms in plan:
        for m in ms:
            tid = c + "." + m
            methods[tid] = {"cpython": cp.get(tid), "mm": mres.get(tid)}
    rec["methods"] = methods
    rec["seconds"] = round(time.monotonic() - t0, 1)
    return rec


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--lib-test", required=True)
    ap.add_argument("--mm", required=True)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--out", default=os.path.join(WORK_ROOT, "raw"))
    ap.add_argument("--only", nargs="*")
    args = ap.parse_args()
    units = []
    with open(os.path.join(HERE, "modules.txt")) as f:
        for ln in f:
            ln = ln.split("#", 1)[0].strip()
            if ln:
                units.append(ln)
    if args.only:
        units = [u for u in units if u in args.only]
    os.makedirs(args.out, exist_ok=True)
    unit_dir = os.path.join(args.out, "units")
    os.makedirs(unit_dir, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as ex:
        futs = {ex.submit(run_unit, u, args.lib_test, args.mm, WORK_ROOT): u for u in units}
        for fut in concurrent.futures.as_completed(futs):
            u = futs[fut]
            try:
                rec = fut.result()
            except Exception as e:
                rec = {"unit": u, "fatal": "harness error: %s: %s" % (type(e).__name__, e), "methods": {}}
            with open(os.path.join(unit_dir, u + ".json"), "w") as f:
                json.dump(rec, f, indent=1, sort_keys=True)
            ms = rec.get("methods", {})
            cpp = sum(1 for v in ms.values() if v["cpython"] and v["cpython"]["status"] in ("pass", "xfail"))
            mmp = sum(1 for v in ms.values() if v["cpython"] and v["cpython"]["status"] in ("pass", "xfail")
                      and v["mm"] and v["mm"]["status"] in ("pass", "xfail"))
            print("%-36s total %5d  cpython %5d  mm %5d  %s" % (u, len(ms), cpp, mmp, rec.get("fatal") or "; ".join(rec.get("mm_fatal") or [])), flush=True)


if __name__ == "__main__":
    main()

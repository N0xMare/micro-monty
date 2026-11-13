#!/usr/bin/env python3
"""Carries an idle session's state from one build to another through ordinary cells (UPG-2).

A snapshot restores only on the build that wrote it (docs/user-guide.md, "Upgrading: drain old
builds"). Between cells a host can do without one: an export cell on the old build prints the
session's globals as a value graph, and import cells on a fresh session of the new build
re-create them. Nothing in the engine changes; this file is the reference implementation, and
`gate.py --carry [--base REV]` is its gate.

**Export** (`export_source`): one cell defines a function, calls it and deletes it. It walks
every global except dunders, the bootstrap's tool functions and the exporter's own names, and
prints records, `ascii()` lists of tuples of str and int only, one line per `CHUNK` bytes,
behind `MARK`, until `budget` bytes have been printed; its last line says where to start the
next export cell and which names it could not carry. Records:

    ('N', id)  ('B', id, 0|1)  ('I', id, hex)  ('F', id, float.hex)  ('C', id, re, im)
    ('S', id, text)  ('Y', id, hex)  ('A', id, hex)        str, bytes, bytearray
    ('Q', id, start, stop, step)                           range, hex
    ('L'|'U'|'E'|'R', id, [ids])  ('D', id, [k, v, ...])   list, tuple, set, frozenset, dict
    ('+', id, more)                                        appends to a long text or id list
    ('M', id, name)                                        a module, by __name__
    ('P', id, name)                                        the global `name` once the host's
                                                           statements have run (below)
    ('=', name, id)                                        a global and its value

Ids number objects, so sharing and cycles survive; floats travel as `float.hex`, so nan, inf
and -0.0 do. A function, class or builtin is carried as a `P` record: by the name it is bound
to, which the host re-creates by running again the last top-level statement that bound it in
the cells it sent (a `def`, `class`, `import` or `from ... import`), or which the bootstrap
defines (the tools). A global that reaches anything else is not carried, and the export says
why: a lambda, closure, bound method, user instance, coroutine, generator, task or a stdlib
instance (`re.Match`, `defaultdict`, `Path`, ...); so is a function whose last binding
statement was not one of those four. A host drains such a session instead (or drops the
names, as this gate does, and checks that no later cell needed them).

**Import** (`import_cells`): on a fresh worker, a cell that runs the bootstrap and creates the
record buffer, cells that append the records as literals, a cell with the binding statements
(imports, then defs and classes in the order the session ran them), and a cell whose function
builds every value (empty containers first, then tuples and frozensets bottom-up, then the
contents) and returns them by name, which plain assignments bind: `globals()` is read-only.
"""

from __future__ import annotations

import ast
import re
import textwrap
import warnings

MARK = '\x1eCARRY '
END = '\x1eCARRY-END '
CHUNK = 32 * 1024
# A cell's print budget is 1 MiB less room for the protocol's header; the export stays under.
BUDGET = 900_000
PIECE = 8192
# Under the engine's 256 KiB source limit with room for the largest record (8192 chars escaped).
SOURCE_CHUNK = 160 * 1024
PREFIX = '__mm_carry'

EXPORT = '''
def __mm_carry_export(skip, start, budget):
    g = globals()
    ids = {}
    keep = []
    recs = []
    dropped = []
    plain = (type(None), bool, int, float, complex, str, bytes, bytearray, range)
    funcs = ('function', 'builtin_function_or_method', 'method_descriptor', 'wrapper_descriptor')

    def pieces(n, tag, text, step):
        recs.append((tag, n, text[:step]))
        for i in range(step, len(text), step):
            recs.append(('+', n, text[i:i + step]))

    def leaf(v, n, root):
        t = type(v)
        if v is None:
            return ('N', n)
        if t is bool:
            return ('B', n, int(v))
        if t is int:
            return ('I', n, hex(v))
        if t is float:
            return ('F', n, v.hex())
        if t is complex:
            return ('C', n, v.real.hex(), v.imag.hex())
        if t is str:
            return ('S', n, v)
        if t is bytes:
            return ('Y', n, v.hex())
        if t is bytearray:
            return ('A', n, v.hex())
        if t is range:
            return ('Q', n, hex(v.start), hex(v.stop), hex(v.step))
        name = t.__name__
        if name == 'module':
            return ('M', n, v.__name__)
        if name in funcs or isinstance(v, type):
            own = getattr(v, '__name__', '')
            if own == '<lambda>':
                return 'lambda'
            if '<locals>' in getattr(v, '__qualname__', ''):
                return 'closure'
            if root in g and g[root] is v:
                return ('P', n, root)
            for other in g:
                if g[other] is v and other[:2] != '__':
                    return ('P', n, other)
            return 'unbound ' + name
        if name in ('coroutine', 'generator', 'async_generator', 'method', 'Task', 'Future'):
            return name
        if t.__module__ == '__main__':
            return 'instance of ' + t.__qualname__
        return 'stdlib ' + t.__module__ + '.' + t.__qualname__

    def visit(root, value):
        local = {}
        new = []
        stack = [value]
        while stack:
            v = stack.pop()
            k = id(v)
            if k in ids or k in local:
                continue
            n = len(ids) + len(local)
            local[k] = n
            keep.append(v)
            t = type(v)
            if t is list or t is tuple or t is set or t is frozenset:
                items = list(v)
                stack.extend(items)
                new.append((t, n, items))
            elif t is dict:
                items = []
                for kv in v.items():
                    items.extend(kv)
                stack.extend(items)
                new.append((t, n, items))
            else:
                rec = leaf(v, n, root)
                if type(rec) is str:
                    return rec
                new.append(rec)
        for k, n in local.items():
            ids[k] = n
        tags = {list: 'L', tuple: 'U', set: 'E', frozenset: 'R', dict: 'D'}
        for rec in new:
            if type(rec[0]) is str:
                if rec[0] in 'SYA':
                    pieces(rec[1], rec[0], rec[2], %(piece)d)
                else:
                    recs.append(rec)
            else:
                pieces(rec[1], tags[rec[0]], [ids[id(c)] for c in rec[2]], 4096)
        recs.append(('=', root, ids[id(value)]))
        return None

    for name in list(g):
        if name in skip or name[:2] == '__' and name[-2:] == '__' or name[:10] == '__mm_carry':
            continue
        why = visit(name, g[name])
        if why is not None:
            dropped.append((name, why))
    lines = []
    line = []
    size = 0
    for rec in recs:
        text = ascii(rec)
        if line and size + len(text) > %(chunk)d:
            lines.append(ascii(line))
            line = []
            size = 0
        line.append(rec)
        size += len(text) + 2
    if line:
        lines.append(ascii(line))
    used = 0
    i = start
    while i < len(lines) and (i == start or used + len(lines[i]) <= budget):
        print(%(mark)r + lines[i])
        used += len(lines[i]) + 8
        i += 1
    print(%(end)r + ascii((i, len(lines), dropped)))
__mm_carry_export(%%r, %%d, %%d)
del __mm_carry_export
'''

IMPORT = '''
def __mm_carry_import(recs):
    g = globals()
    tags = {}
    data = {}
    binds = []
    for r in recs:
        tag = r[0]
        if tag == '+':
            data[r[1]] += r[2]
        elif tag == '=':
            binds.append((r[1], r[2]))
        else:
            tags[r[1]] = tag
            data[r[1]] = r[2] if len(r) == 3 else r[2:]
    vals = {}
    later = []
    for n, tag in tags.items():
        d = data[n]
        if tag == 'N':
            vals[n] = None
        elif tag == 'B':
            vals[n] = bool(d)
        elif tag == 'I':
            vals[n] = int(d, 16)
        elif tag == 'F':
            vals[n] = float.fromhex(d)
        elif tag == 'C':
            vals[n] = complex(float.fromhex(d[0]), float.fromhex(d[1]))
        elif tag == 'S':
            vals[n] = d
        elif tag == 'Y':
            vals[n] = bytes.fromhex(d)
        elif tag == 'A':
            vals[n] = bytearray.fromhex(d)
        elif tag == 'Q':
            vals[n] = range(int(d[0], 16), int(d[1], 16), int(d[2], 16))
        elif tag == 'M':
            m = __import__(d)
            for part in d.split('.')[1:]:
                m = getattr(m, part)
            vals[n] = m
        elif tag == 'P':
            vals[n] = g[d]
        elif tag == 'L':
            vals[n] = []
        elif tag == 'D':
            vals[n] = {}
        elif tag == 'E':
            vals[n] = set()
        else:
            later.append(n)
    for n in later:
        stack = [n]
        while stack:
            m = stack[-1]
            if m in vals:
                stack.pop()
                continue
            need = [c for c in data[m] if c not in vals]
            if need:
                stack.extend(need)
                continue
            items = [vals[c] for c in data[m]]
            vals[m] = tuple(items) if tags[m] == 'U' else frozenset(items)
            stack.pop()
    for n, tag in tags.items():
        d = data[n]
        if tag == 'L':
            vals[n].extend([vals[c] for c in d])
        elif tag == 'E':
            vals[n].update([vals[c] for c in d])
        elif tag == 'D':
            v = vals[n]
            for i in range(0, len(d), 2):
                v[vals[d[i]]] = vals[d[i + 1]]
    return {name: vals[n] for name, n in binds}
__mm_carry_values = __mm_carry_import(__mm_carry_parts)
'''


def bootstrap_of(script):
    """A session's first bootstrap (None if it has none) and the names it defines."""
    for frame in script:
        boot = frame.get('prelude', {}).get('bootstrap') if frame['frame'] == 'start_cell' else None
        if boot:
            names = set(re.findall(r'^async def (\w+)', boot['source'], re.M))
            return boot, names | {b['function'] for b in boot['bindings']}
    return None, set()


def export_source(skip, start: int, budget: int = BUDGET) -> str:
    body = EXPORT % {'piece': PIECE, 'chunk': CHUNK, 'mark': MARK, 'end': END}
    return body % (sorted(skip), start, budget)


def parse_export(output: str):
    """The records and the end line (next start, line count, dropped) of one export cell."""
    records, end = [], None
    for line in output.split("\n"):
        if line.startswith(MARK):
            records.extend(ast.literal_eval(line[len(MARK):]))
        elif line.startswith(END):
            end = ast.literal_eval(line[len(END):])
    return records, end


class Sources:
    """The last top-level statement that bound each name, over the cells a session ran."""

    def __init__(self):
        self.last = {}
        self.order = 0

    def add(self, source: str):
        try:
            with warnings.catch_warnings():
                warnings.simplefilter('ignore')
                tree = ast.parse(source)
        except SyntaxError:
            return
        lines = source.splitlines(keepends=True)
        self._body(tree.body, lines)

    def _body(self, body, lines):
        for stmt in body:
            self.order += 1
            if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                first = min([stmt.lineno] + [d.lineno for d in stmt.decorator_list])
                text = textwrap.dedent(''.join(lines[first - 1:stmt.end_lineno]))
                self.last[stmt.name] = (self.order, text)
            elif isinstance(stmt, ast.Import):
                for a in stmt.names:
                    if a.asname:
                        self.last[a.asname] = (self.order, f'import {a.name} as {a.asname}\n')
                    else:
                        top = a.name.split('.')[0]
                        self.last[top] = (self.order, f'import {a.name}\n')
            elif isinstance(stmt, ast.ImportFrom):
                for a in stmt.names:
                    name = a.asname or a.name
                    if stmt.level == 0 and a.name != '*':
                        self.last[name] = (self.order,
                                           f'from {stmt.module} import {a.name} as {name}\n')
                    else:
                        self.last.pop(name, None)
            else:
                for node in ast.walk(stmt):
                    if isinstance(node, ast.Name) and not isinstance(node.ctx, ast.Load):
                        self.last.pop(node.id, None)
                    elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef,
                                           ast.ClassDef)):
                        self.last.pop(node.name, None)
                    elif isinstance(node, (ast.Import, ast.ImportFrom)):
                        for a in node.names:
                            self.last.pop((a.asname or a.name).split('.')[0], None)
                # Statements under a top-level `if`, `try`, `with`, `for` or `while` run at the
                # top level too: their binding statements count.
                for field in ('body', 'orelse', 'finalbody'):
                    inner = getattr(stmt, field, None)
                    if isinstance(inner, list) and not isinstance(stmt, ast.ClassDef):
                        self._body(inner, lines)
                for handler in getattr(stmt, 'handlers', []):
                    self._body(handler.body, lines)


def plan(records, dropped, sources: Sources, tools):
    """The records to send, the binding statements, and what could not be carried.

    A `P` record whose name has no binding statement (and is not a tool) drops every global
    that reaches it."""
    dropped = {name: why for name, why in dropped}
    needs = {r[2] for r in records if r[0] == 'P'}
    missing = {n for n in needs if n not in tools and n not in sources.last}
    if missing:
        # The globals whose graph reaches a missing name: walk each root's ids.
        kids, kind = {}, {}
        for r in records:
            if r[0] in ('L', 'U', 'E', 'R', 'D'):
                kids[r[1]] = list(r[2])
            elif r[0] == '+' and r[1] in kids:
                kids[r[1]].extend(r[2])
            if r[0] == 'P':
                kind[r[1]] = r[2]
        roots = [(r[1], r[2]) for r in records if r[0] == '=']
        bad = set()
        for name, n in roots:
            stack, seen = [n], set()
            while stack:
                m = stack.pop()
                if m in seen:
                    continue
                seen.add(m)
                if kind.get(m) in missing:
                    bad.add(name)
                    dropped[name] = f'no binding statement for {kind[m]}'
                    break
                stack.extend(kids.get(m, ()))
        # Unbound, the missing names' nodes stay as None (nothing reaches them).
        records = [('N', r[1]) if r[0] == 'P' and r[2] in missing else r for r in records
                   if not (r[0] == '=' and r[1] in bad)]
        needs = {n for n in needs if n not in missing}
    statements = sorted(sources.last[n] for n in needs if n not in tools and n in sources.last)
    return records, ''.join(text for _, text in statements), dropped


def import_cells(records, statements: str, names) -> list[str]:
    """The sources of the import cells after the first (which creates the buffer)."""
    cells, chunk, size = [], [], 0
    for r in records:
        text = ascii(r)
        if chunk and size + len(text) > SOURCE_CHUNK:
            cells.append(f'{PREFIX}_parts += {ascii(chunk)}\n')
            chunk, size = [], 0
        chunk.append(r)
        size += len(text) + 2
    if chunk:
        cells.append(f'{PREFIX}_parts += {ascii(chunk)}\n')
    if statements:
        cells.append(statements)
    binds = ''.join(f'{n} = {PREFIX}_values[{n!r}]\n' for n in names)
    cells.append(IMPORT + binds + f'del {PREFIX}_values, {PREFIX}_import, {PREFIX}_parts\n')
    return cells

# The ast module on the own parser: parse (exec and eval), dump with and
# without attributes and indent, walk, NodeVisitor, NodeTransformer,
# literal_eval, get_docstring, get_source_segment, iter_fields, the location
# helpers, node construction and repr, and SyntaxError attributes. The
# patterns are those models used in real cells: parse a file's text with a
# filename, walk it and count node types.
import ast

SRC = '''"""Module doc.

    More text.
"""
import os, sys as system
from collections import namedtuple as nt, OrderedDict


def f(a, /, b: int = 1, *args, c, d=2, **kw) -> str:
    """Return a string.

    Indented details.
    """
    try:
        x = a + b * 2
    except (ValueError, TypeError) as e:
        raise RuntimeError("bad") from e
    finally:
        pass
    return f"{x!r:>{b}} and {a}"


async def g():
    async with ctx() as c:
        await c
    async for i in aiter():
        yield i


class C(Base, metaclass=Meta):
    """Doc of C."""
    attr: int = 3

    @property
    def p(self):
        return [i ** 2 for i in range(10) if i % 2]


while x < 10 <= y and not z or w:
    x += 1
    if x:
        break
    elif y:
        continue
    else:
        del x[1:2, ::3]
lam = lambda q, *r: {q: r, **{}}
s = {1, 2}; t = (1,); u = -1; v = ~x; w = b"bytes"
match cmd:
    case [1, *rest]:
        pass
    case {"k": v, **others}:
        pass
    case Point(x=0) | None:
        pass
    case str() as name if name:
        pass
global_ = (yield_ := 5)
'''

tree = ast.parse(SRC, filename='example.py')
print(type(tree).__name__, len(tree.body))
print(ast.dump(tree.body[1]))
print(ast.dump(tree.body[2], include_attributes=True))
print(ast.dump(tree.body[3], indent=2))
print(ast.dump(tree.body[-1], annotate_fields=False))

# Count node types the way the real cells do.
counts = {}
for node in ast.walk(tree):
    name = type(node).__name__
    counts[name] = counts.get(name, 0) + 1
for name in sorted(counts):
    print(name, counts[name])
print(sum(isinstance(n, (ast.Raise, ast.Try)) for n in ast.walk(tree)))
print([n.name for n in ast.walk(tree)
       if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef))])

# Positions and source segments.
for node in tree.body[3].body:
    print(type(node).__name__, node.lineno, node.col_offset,
          node.end_lineno, node.end_col_offset)
ret = tree.body[3].body[-1]
print(repr(ast.get_source_segment(SRC, ret)))
print(repr(ast.get_source_segment(SRC, ret.value, padded=True)))
fstr = ret.value
print([(type(v).__name__, v.col_offset, v.end_col_offset) for v in fstr.values])

# Docstrings.
print(repr(ast.get_docstring(tree)))
print(repr(ast.get_docstring(tree.body[3])))
print(repr(ast.get_docstring(tree.body[3], clean=False)))
print(repr(ast.get_docstring(tree.body[5])))
print(repr(ast.get_docstring(tree.body[4])))
try:
    ast.get_docstring(tree.body[1])
except TypeError as e:
    print('TypeError', e)


class Names(ast.NodeVisitor):
    def __init__(self):
        self.names = []

    def visit_Name(self, node):
        self.names.append((node.id, type(node.ctx).__name__))

    def visit_FunctionDef(self, node):
        self.names.append(('def', node.name))
        self.generic_visit(node)


v = Names()
v.visit(tree)
print(v.names[:12], len(v.names))


class Double(ast.NodeTransformer):
    def visit_Constant(self, node):
        if isinstance(node.value, int) and not isinstance(node.value, bool):
            return ast.copy_location(ast.Constant(node.value * 2), node)
        return node

    def visit_Pass(self, node):
        return None


expr = ast.parse('x = [1, 2, (3, 4)]\nif y:\n    pass\n    z = 5')
expr = ast.fix_missing_locations(Double().visit(expr))
print(ast.dump(expr))

# Eval mode and fields.
e = ast.parse('a.b(c, *d, k=1)[2]', mode='eval')
print(ast.dump(e))
print(list(ast.iter_fields(e.body.value)))
print([type(n).__name__ for n in ast.iter_child_nodes(e.body.value)])
print(ast.Call._fields, ast.arguments._fields, ast.Name._attributes)

# Node construction, repr and comparison helpers.
n = ast.BinOp(left=ast.Constant(1), op=ast.Add(), right=ast.Name('x', ast.Load()))
print(ast.dump(n))
print(repr(n))
print(repr(ast.parse('f(1, [2, [3, [4]]])').body[0]))
m = ast.Module(body=[ast.Expr(n)], type_ignores=[])
ast.fix_missing_locations(m)
print(n.lineno, n.col_offset, n.end_lineno, n.end_col_offset)
ast.increment_lineno(m, 3)
print(n.lineno, n.left.lineno)
print(isinstance(ast.Add(), ast.operator), issubclass(ast.Expr, ast.stmt),
      issubclass(ast.Name, ast.AST))

# literal_eval.
for text in ['1', '-2.5', '1+2j', "'s'", "b'x'", '[1, (2, 3), {4: 5}]',
             '{1, 2}', 'set()', 'None', 'True', '  (1,)', '-(1)', '3 - 1j',
             "{'a': [1, -2]}", '()']:
    print(repr(ast.literal_eval(text)))
print(ast.literal_eval(ast.parse('[1, 2]', mode='eval')))
for bad in ['x', 'f()', '1 + x', '[1, a]', '2 * 3', '{**a}', '1 if 2 else 3']:
    try:
        ast.literal_eval(bad)
    except ValueError as err:
        print('ValueError', err)
try:
    ast.literal_eval('1 +')
except SyntaxError as err:
    print('SyntaxError', err.msg, err.lineno, err.offset)

# SyntaxError attributes from parse.
for text in ['x = (', 'def f(:\n  pass', 'if x\n  y', 'x = "abc',
             'for x in y\n  pass', '  x = 1', 'return +', 'a b']:
    try:
        ast.parse(text, filename='bad.py')
    except SyntaxError as err:
        print(type(err).__name__, repr(err.msg), err.filename, err.lineno,
              err.offset, repr(err.text), err.end_lineno, err.end_offset)
try:
    ast.parse('x', mode='bogus')
except ValueError as err:
    print('ValueError', err)

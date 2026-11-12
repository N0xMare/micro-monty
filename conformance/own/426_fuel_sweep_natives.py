# Natives the fuel sweep changed: class versions and dunder flags across
# deep and wide hierarchies (single-base classes inherit their base's
# flags), the C3 merge, columns on non-ASCII lines, traceback source
# lines, int/complex error text, and a container repr long enough to go on
# in the prelude.
import ast
import io
import sys
import tokenize
import traceback


class B:
    pass


chain = [B]
for i in range(60):
    chain.append(type('S' + str(i), (chain[-1],), {'__len__': (lambda self: 7)} if i == 20 else {}))
wide = [type('W' + str(i), (B,), {}) for i in range(200)]


class M(chain[10], int):
    pass


class N(chain[30], chain[5]):
    pass


x, m, n, w = chain[-1](), M(3), N(), wide[-1]()


def show(tag):
    print(tag, x == x, x == 1, m == 3, n == 1, w == 1, len(x), bool(chain[3]()), hash(m) == hash(3), repr(x)[:3], len(n))


show('start')
B.__eq__ = lambda a, b: True
show('eq')
B.__len__ = lambda self: 0
chain[25].__len__ = lambda self: 2
show('len')
del B.__eq__
show('deleq')
chain[40].__repr__ = lambda self: 'R40'
B.__bool__ = lambda self: False
show('bool')
B.__add__ = lambda a, b: 'add'
print(x + 1, n + 1, m + 1, w + 1)
del chain[25].__len__
show('dellen')
B.y = 5
print(x.y, w.y, n.y)


class O: pass
class A(O): pass
class C(O): pass
class D(O): pass
class E(O): pass
class K1(A, C, D): pass
class K2(E, C): pass
class K3(E, A): pass
class Z(K1, K2, K3): pass


print([k.__name__ for k in Z.__mro__])
print([k.__name__ for k in type('X', (int, A), {}).__mro__])
for bases in [(A, O), (K1, A, K3), (C, A, K1)]:
    try:
        type('T', bases, {})
        print('ok', [b.__name__ for b in bases])
    except TypeError as e:
        print(e)

src = 'é = "ü" + f"{é!r:>{3}}ß" ;  y = [1, "€", (é,)]  # cömment\nz = 1\nw = "x" if é else b"x"\n'
for node in ast.walk(ast.parse(src)):
    if hasattr(node, 'lineno'):
        print(type(node).__name__, node.lineno, node.col_offset, node.end_lineno, node.end_col_offset)
print(ast.get_source_segment(src, ast.parse(src).body[1].value))
for t in list(tokenize.generate_tokens(io.StringIO(src).readline))[:14]:
    print(t.type, repr(t.string), t.start, t.end)
long = 'x = [' + ', '.join(['"é"'] * 3000) + ']\n'
tree = ast.parse(long)
print(tree.body[0].value.elts[-1].col_offset, tree.body[0].value.end_col_offset)


def g(v):
    return (v +
            1) / 0


def f(k):
    if k == 0:
        return g(
            k)
    return f(k - 1)


try:
    f(5)
except ZeroDivisionError:
    print(traceback.format_exc().count('\n'))
    for fs in traceback.extract_tb(sys.exc_info()[2]):
        print(fs.lineno, fs.name, fs.line)

for text in ['1' * 300 + 'x', "'" * 250 + '"', 'é' * 230 + '\n', ' 1_000 _']:
    try:
        int(text)
    except ValueError as e:
        print(e)
try:
    int(b'\xff' * 300)
except ValueError as e:
    print(e)
try:
    complex('1' * 3000 + 'j' * 2)
except ValueError as e:
    print(e)
xs = [[i, (i,)] for i in range(50_000)]
t = str(xs)
print(len(t), t[:20], t[-20:], repr({i: [i] for i in range(45_000)})[-15:])

# Rarely reached paths: hash and comparison errors, layout conflicts,
# short byte searches, Unicode float text, grouped fraction formats,
# throw through yield from, %a of user reprs and OSError's str.

class H:
    def __hash__(self):
        return 'x'
try:
    hash(H())
except TypeError as e:
    print(e)
class A:
    pass
for f in (lambda: [A()] < [A()], lambda: -A(), lambda: A() + A(), lambda: (A(),) <= (A(),), lambda: sorted([A(), A()])):
    try:
        f()
    except TypeError as e:
        print(e)

for bases in ((list, dict), (int, str), (list, tuple)):
    try:
        type('C', bases, {})
    except TypeError as e:
        print(e)

for h, n in ((b'ab', b'ab'), (b'abc', b'bc'), (b'xyzxyz', b'zx'), (b'a' * 40, b'aaab'), (b'ab' * 30 + b'c', b'abc')):
    print(h.find(n), h.rfind(n), h.count(n), bytearray(h).find(n), h.rfind(n, 1), h.index(n) if n in h else -1)

for s in ('١٢٣', '\u0663.\u0661', '１.５e３', '\u00a0 7', '٣٫٥'):
    try:
        print(float(s))
    except ValueError as e:
        print(e)

for spec in ('._f', '.,f', ',._f', '_.4_f', '.3_f', '10.,f', ',.10_f'):
    try:
        print(spec, format(1234.567891, spec))
    except ValueError as e:
        print(spec, e)

def inner():
    yield 1
def outer():
    yield from inner()
g = outer()
next(g)
try:
    g.throw(KeyError('k'))
except KeyError as e:
    print('KeyError', e)
class It:
    def __iter__(self): return self
    def __next__(self): return 1
def o2():
    yield from It()
g = o2()
next(g)
try:
    g.throw(ValueError('v'))
except ValueError as e:
    print('ValueError', e)

class R:
    def __repr__(self):
        return 'é!'
print('%a' % R(), '%a|%a' % ([R()], {1: R()}), b'%a' % [R()])

class E(OSError):
    pass
print(str(E(2, 'x', 'f')), str(OSError(2, 'x', 'f', None, 'g')), str(FileNotFoundError(2, 'y')), str(E(1, 2, 3, 4, 5, 6)))

def f(a, /, **k):
    return a, k
print(f(1, a=2))
def g(**k): return k
try:
    g(**{'a': 1}, a=2)
except TypeError as e:
    print(e)
def h(*, a, **k): return a, k
try:
    h(a=1, **{'a': 2})
except TypeError as e:
    print(e)

class I:
    def __index__(self):
        return 3
class L(list):
    pass
print(L([1]) * I(), I() * L([2]), L([1]) + [2], L([3]) == [3], L([1]) < [2])

x, y = [{}, 1], [{}, 2]
print(x < y, x <= y, [{1: 2}, 0] < [{1: 2}, 1], ([], {}) < ([], {}) if False else None)
try:
    print([{}] < [{}])
except TypeError as e:
    print(e)

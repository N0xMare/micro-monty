# The interpreter's fast paths must not be observable: cls(...) through a
# call site's cached __init__, generator resume and yield, str methods and
# small-int formatting.

# cls(...) with a plain __init__, each call site run several times so its
# cache fills, then the class changes under it.
class P:
    def __init__(self, x, y):
        self.x = x
        self.y = y


def make(n):
    return [P(i, -i) for i in range(n)]


print([(p.x, p.y) for p in make(3)])


def init2(self, x, y):
    self.x = x * 10
    self.y = y * 10


P.__init__ = init2
print([(p.x, p.y) for p in make(3)])


class Q(P):
    pass


def make_q():
    return [vars(Q(i, i)) for i in range(3)]


print(make_q())


def init3(self, x, y):
    self.sum = x + y


P.__init__ = init3
print(make_q())
Q.__init__ = lambda self, x, y: setattr(self, 'lam', (x, y))
print(make_q())
del Q.__init__
print(make_q())


def with_new(cls, x, y):
    o = object.__new__(cls)
    o.via_new = True
    return o


P.__new__ = with_new
print(make_q(), [vars(p) for p in make(2)])


class R:
    def __init__(self, v):
        self.v = v
        if v == 3:
            raise KeyError(v)
        if v == 4:
            return v


for v in range(6):
    try:
        print(R(v).v)
    except (KeyError, TypeError) as e:
        print(type(e).__name__, e)
for args in [(), (1, 2)]:
    for _ in range(3):
        try:
            R(*args)
        except TypeError as e:
            print(e)


class Meta(type):
    calls = 0

    def __call__(cls, *a):
        Meta.calls += 1
        return super().__call__(*a)


class M(metaclass=Meta):
    def __init__(self, v):
        self.v = v


print([M(i).v for i in range(3)], Meta.calls)
Meta.__call__ = lambda cls, *a: ('meta', a)
print([M(i) for i in range(2)])


class Big:
    def __init__(self, a):
        for i in range(40):
            setattr(self, f'a{i}', a + i)


print(sum(vars(Big(1)).values()), len(vars(Big(2))))


# Generators: resume and yield, send, throw, close, return values, nesting.
def gen(n):
    total = 0
    for i in range(n):
        got = yield i
        if got:
            total += got
    return total


g = gen(4)
print(next(g), g.send(10), g.send(5), next(g))
try:
    next(g)
except StopIteration as e:
    print('stop', e.value)
print(list(gen(5)), sum(gen(100)), [x for x in gen(3)], list(zip(gen(3), gen(4))))


def outer():
    r = yield from gen(3)
    yield ('inner returned', r)


print(list(outer()))
g = gen(10)
next(g)
try:
    g.throw(ValueError('boom'))
except ValueError as e:
    print('thrown', e)
print(next(g, 'exhausted'))
g = gen(10)
next(g)
g.close()
print(next(g, 'closed'))


def running():
    yield g2.gi_running


g2 = running()
print(list(g2), g2.gi_running)


def fin():
    try:
        yield 1
        yield 2
    finally:
        print('finally ran')


for x in fin():
    print(x)
print(list(x * 2 for x in range(4)), dict((k, k) for k in 'ab'))

# str methods and formatting on small and large ints and floats.
for n in [0, 7, -7, 42, -128, 2**31, -2**63, 10**20]:
    print(f'{n}|{n:5}|{n:<5}|{n:^7}|{n:05}|{n:+}|{n:x}|{n:#o}|{n:b}|{n:,}|{n:_}', '%d|%5d|%-5d|%05d|%+d|%x|%X|%o|%s|%r' % ((n,) * 10))
for x in [0.5, -1.25, 1e100, float('inf')]:
    print(f'{x}|{x:.2f}|{x:10.3e}|{x:g}', '%.3f|%10.2e|%g|%s' % (x, x, x, x))
s = 'Hello, World'
print(s.startswith('Hell'), s.endswith(('x', 'ld')), s.find('o'), s.rfind('o'), s.count('l'), s.isalpha(), 'abc'.isalpha(), '12'.isdigit(),
      s.lower(), s.upper(), s.split(', '), '-'.join(['a', 'b']), s == 'Hello, World', s != 'x', 'é' == 'é', 'a' < 'b', s.replace('l', 'L', 2))
print('%s and %s' % ('x', 'y'), '%(a)s=%(b)d' % {'a': 'k', 'b': 3}, '%c%c' % (65, 'b'), '%%', '%5s|%-5s|%.2s' % ('ab', 'ab', 'abc'))

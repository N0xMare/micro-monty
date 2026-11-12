# enumerate's count goes on past 2**63 - 1 into big ints, as CPython's does
# (en_longindex), and a start past it is taken as is: the count no longer
# wraps negative or raises OverflowError.
M = 2**63 - 1

print(list(enumerate('abc', M)))
print(list(enumerate('abc', M - 1)))
print(list(enumerate('ab', 2**64)), list(enumerate('ab', -2**70)))
print(list(enumerate('ab', -M - 1)), list(enumerate('ab', True)))
e = enumerate(iter([1, 2, 3]), M)
print(next(e), next(e), next(e), next(e, 'end'))


class I(int):
    pass


print(list(enumerate('ab', I(2**64))), list(enumerate('ab', I(M))))
try:
    enumerate('a', 1.5)
except TypeError as x:
    print('TypeError', x)

# Read nested, zipped and by folds.
print(list(enumerate(enumerate('ab', M), M)))
print(list(zip(enumerate('abc', M), enumerate('abc', M - 1))))
print(sum(i for i, _ in enumerate('abcd', M)) - 4 * M)
print(max(enumerate('abc', M)), sorted(enumerate('ab', M))[-1])

# Read in place by dict and set builds, the count passing 2**63 - 1 at
# every item of the first 40, and around the items where a growing table
# makes the build read an item again.
for k in range(40):
    d = dict(enumerate(range(40), M - k))
    s = set(enumerate(range(40), M - k))
    f = dict.fromkeys(enumerate(range(20), M - k))
    z = dict(zip(enumerate(range(40), M - k), range(40)))
    ok = list(d) == [M - k + i for i in range(40)]
    ok = ok and sorted(s) == sorted(d.items()) and len(f) == 20 and len(z) == 40
    print(k, ok, max(s)[0] - M, max(z)[0] - M)
for at in (4914, 8192):
    for off in (-1, 0, 1, 2):
        start = M - at + off
        n = at + 50
        d = dict(enumerate(range(n), start))
        s = set(enumerate(range(n), start))
        want = [start + i for i in range(n)]
        print(at, off, list(d) == want, sorted(k for k, _ in s) == want)

# Every kind of start, on a builtin iterator (read natively) and on other
# iterables (the prelude twin), the start read before the iterable as
# CPython's PyNumber_Index reads it: an exact int, or TypeError at the call.
log = []


class X:
    def __init__(self, v):
        self.v = v

    def __index__(self):
        log.append('index')
        return self.v


class It:
    def __iter__(self):
        log.append('iter')
        return iter('ab')


class Sub(int):
    pass


class SubIdx(int):
    def __index__(self):
        return 99


def gen():
    yield 'a'
    yield 'b'


starts = [5, -3, True, False, X(7), X(2**70), X('s'), 1.5, 'a', None, [], 2**64, M,
          -2**70, Sub(4), Sub(2**65), SubIdx(3)]
for mk in (lambda: 'abc', lambda: [1, 2], It, gen, lambda: 5):
    for st in starts:
        log.clear()
        try:
            e = enumerate(mk(), st)
        except TypeError as x:
            print('call', x, log)
            continue
        try:
            print([(type(k).__name__, k) for k, _ in e], log)
        except TypeError as x:
            print('next', x, log)
for src in ('ab', It()):
    for st in (3, X(4), 1.5, True, Sub(2**64)):
        try:
            print(list(enumerate(src, start=st)))
        except TypeError as x:
            print(type(x).__name__, x)

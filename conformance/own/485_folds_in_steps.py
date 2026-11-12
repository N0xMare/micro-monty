# Bounded native steps (docs/spec.md §5.1): `any`, `all`, `min`, `max` and
# `sum` over a container's iterator fold a step at a time (`nstep::op::FOLD`)
# and hand the rest to Python at an item that needs it; a long tuple hashes
# a slice per call (`_thash`); collecting a str, set or dict goes on a
# chunk per step (`_collect_rest`). Results must equal one native run's.


class B:
    def __init__(self, v):
        self.v = v

    def __bool__(self):
        print('bool', self.v)
        return bool(self.v)

    def __lt__(self, o):
        print('lt', self.v)
        return self.v < (o.v if isinstance(o, B) else o)

    def __gt__(self, o):
        print('gt', self.v)
        return self.v > (o.v if isinstance(o, B) else o)

    def __radd__(self, o):
        print('radd', self.v)
        return o + self.v

    def __hash__(self):
        print('hash', self.v)
        return self.v * 7

    def __repr__(self):
        return f'B({self.v})'


class Bad:
    def __gt__(self, o):
        raise KeyError('bad gt')


xs = list(range(200_000))
print(min(xs), max(xs), sum(xs), any(xs), all(xs), all(xs[1:]))
print(max(xs + [B(200_001), 3]), min([B(5)] + xs[1:]), sum(xs + [B(4), 5]))
print(any([0] * 150_000 + [B(0), B(2)]), all([1] * 150_000 + [B(1), B(0), B(3)]))
print(sum([0.1] * 150_000), sum([1, 0.1, 2] * 100_000), sum(xs, 0.5))
print(sum([1e100, 1.0, -1e100] * 100_000), sum([0.5] * 200_000 + [10**30] + [0.25] * 200_000))
print(max('hello world' * 40_000), min(set(range(5, 150_000))), max(dict.fromkeys(xs)), sum(dict.fromkeys(xs)))
print(max(iter(xs)), sum(iter(range(10**6))), any(range(150_000, 0, -1)), all(b'\x01' * 150_000))
print(min([], default=7), max(iter([]), default=None), max(xs, default=-1))
for f in (min, max):
    try:
        f(iter(()))
    except ValueError as e:
        print(e)
try:
    max(xs + ['a'])
except TypeError as e:
    print(e)
try:
    sum(xs + ['a'])
except TypeError as e:
    print(e)
try:
    max(xs + [Bad()])
except KeyError as e:
    print('KeyError', e)
d = dict.fromkeys(range(150_000))
it = iter(d)
print(next(it), sum(it))
t = tuple(xs)
print(hash(t) == hash(tuple(range(200_000))), hash(t[:20_000]), hash((t[:20_000], t[:17_000])))
print(hash(tuple(xs[:200_000] + [B(4)] + xs[:200_000])))
s = (t[:30_000],) * 3
print(hash((s, s, s)) == hash(((t[:30_000],) * 3,) * 3), {t[:20_000]: 1}[tuple(range(20_000))])
try:
    hash(t[:200_000] + ([2],))
except TypeError as e:
    print(e)
u = 'abcdé' * 60_000
print(len(list(u)), len(tuple(u)), sorted(set(u)), list(u)[-3:], ''.join(u) == u)
print(len(list(set(range(150_000)))), sorted(dict.fromkeys(u)), len(list(dict.fromkeys(xs).items())))
a, *b = u
print(a, len(b), [*u][:3], {*u} == set('abcdé'), len(bytes(list(b'xy' * 200_000))))


# A fold takes each item as it is pulled (CPython's order): an item that
# fails stops it there, and the iterator keeps the rest.
def loud(items):
    for x in items:
        print('yield', repr(x))
        yield x


class Loud:
    def __init__(self, items):
        self.items = list(items)

    def __iter__(self):
        return self

    def __next__(self):
        if not self.items:
            raise StopIteration
        return self.items.pop(0)


for fold, items in ((sum, [1, 2, 'a', 4]), (min, [3, 1, 'a', 2]), (max, [3, 1, 'a', 2]),
                    (lambda it: sum(it, 0.5), [1, 0.25, 'a', 4]), (lambda it: sum(it, []), [[1], 2, [3]])):
    for it in (loud(items), Loud(items), iter(items), zip(items, loud(items))):
        try:
            print(fold(it))
        except TypeError as e:
            print('TypeError', e)
        print('left', list(it))
# sum's stages as CPython's: ints in i64, then floats and complex numbers
# compensated, then `+`, never back.
for items in ([1e16, 1, 1, 1, 1], [0.1, True, 0.1, 0.2, 0.3], [1j, 0.1, 0.1j, 0.2, 0.3], [2**64, -1.8446744073709552e19, 0.1, 0.2, 0.3],
              [1, 2, 2**62, 2**62, 0.1, 0.2], [0.5, 2**64, 1e-20], [1e308, 1e308, -1e308], [0.1, 1j, 0.2, 0.3]):
    print(sum(items), sum(iter(items)), sum(loud(items)) if len(items) < 3 else sum(x for x in items), sum(items, 0.0))
print(sum(x > 2 for x in range(10)), sum((0.1 for _ in range(10)), 1), sum(loud([]), 7))
# A total that leaves i64 and comes back is added plainly, as CPython's
# int stage ends there.
for items in ([2**62, 2**62, -2**62, -2**62, 0.1, 1e16, 0.1, -1e16], [2**63, -2**63, 0.1, 1e16, 0.1, -1e16],
              [2**62, 2**62, -2**63, 1e16, 1.0, 1.0, 1.0, 1.0]):
    print(sum(items), sum(iter(items)), sum(x for x in items), sum(items, 2**64))


# Containers, bytes and math folds also take each item as it is pulled,
# from a generator or a builtin iterator alike: the first bad item raises
# and the iterator keeps the rest.
class K:
    def __init__(self, v):
        self.v = v

    def __hash__(self):
        return hash(self.v)

    def __eq__(self, o):
        return isinstance(o, K) and o.v == self.v

    def __repr__(self):
        return f'K({self.v})'


import math
H = [1, 2, K(3), [4], 5, 6]
P = [(1, 2), (3, 4), (K(5), 6), 7, (8, 9)]
N = [1, 2, 300, 4, 5]
F = [1, 2.5, 'a', 4, 5]
for name, f, items in (('set', set, H), ('frozenset', frozenset, H), ('display', lambda it: {*it}, H),
                       ('update', lambda it: set().update(it), H), ('union', lambda it: frozenset().union(it), H),
                       ('difference', lambda it: {1}.difference(it), H), ('symdiff', lambda it: {1}.symmetric_difference(it), H),
                       ('ixor', lambda it: {1}.symmetric_difference_update(it), H),
                       ('difference_update', lambda it: {1}.difference_update(it), H),
                       ('dict', dict, P), ('dict.update', lambda it: {}.update(it), P), ('fromkeys', dict.fromkeys, H),
                       ('bytes', bytes, N), ('bytearray', bytearray, N), ('extend', lambda it: bytearray().extend(it), N),
                       ('fsum', math.fsum, F), ('prod', math.prod, F), ('sumprod', lambda it: math.sumprod(it, F), F)):
    for src in (lambda: (x for x in items), lambda: iter(items), lambda: zip(items), lambda: reversed(items)):
        it = src()
        try:
            r = f(it)
        except (TypeError, ValueError) as e:
            r = type(e).__name__
        print(name, r, list(it))
# Long iterators are read in place a step at a time; an item that needs
# Python code, or one that waits for a bigger table, is read again.
n = 150_000
it = iter(list(range(n)) + [K(-1)] + list(range(n, 2 * n)))
s = set(it)
print(len(s), K(-1) in s, list(it))
it = zip(range(n), range(n, 2 * n))
d = dict(it)
print(len(d), d[n - 1], list(it))
e = dict(enumerate(['a', 'b'] * n))
print(len(e), e[2 * n - 1], len(set(reversed(range(n)))), len(dict.fromkeys(iter(range(n)))))
it = iter(list(range(n)) + [[0], 7, 8])
try:
    set().union(it)
except TypeError as ex:
    print('TypeError', ex, list(it))
s = set(range(10))
try:
    s.difference_update(iter(s))
except RuntimeError as ex:
    print('RuntimeError', ex)
r = iter(range(0, 100, 7))
print(r.__length_hint__(), next(r), next(r), r.__length_hint__(), list(r)[-1], r.__length_hint__())
print(iter(range(5, -5, -3)).__length_hint__(), reversed(range(10)).__length_hint__())

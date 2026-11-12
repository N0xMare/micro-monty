# Bounded native steps for containers (docs/spec.md §5): set and dict
# algebra, `dict(...)`, `update`, `{**d}`, copies and views (a range is
# read in place) go on a slice of items per step in `_steps`
# (`nstep::op::BULK`), handing the rest to `_bulk` once an item needs
# Python code; lists and tuples built from slices of
# others (`+`, `*`, `extend`, slicing, slice assignment, `insert`, `del`)
# a batch per step (`SEQ_BUILD`); `count`, `index` and `remove` scan a
# slice per step (`SEQ_SCAN`, then `_seq_scan_rest`); set `==` and `<=`
# go on in `_ssub` a native slice (`_more`) per loop step. Results must equal
# one native run's.


class K:
    # Equal to the int it wraps, so only Python code can find it.
    def __init__(self, v):
        self.v = v

    def __hash__(self):
        return hash(self.v)

    def __eq__(self, o):
        return o == self.v

    def __repr__(self):
        return f'K({self.v})'


def digest(c):
    return len(c), sum(x if type(x) is int else 0 for x in c)


N = 200_000
a = set(range(N))
b = set(range(N // 2, N + N // 2))
print(digest(a & b), digest(a | b), digest(a - b), digest(a ^ b), digest(b - a))
x = set(a)
y = x
x &= b
x ^= {1, 2, N + 7}
x -= set(range(N // 2, N // 2 + 10))
x |= {-5}
print(x is y, digest(y))
f = frozenset(a)
print(type(f & b).__name__, type(f | b).__name__, type(b - f).__name__, digest(f ^ b))
print(a == set(range(N)), a <= b, a >= set(range(10)), f == a, a < set(range(N + 1)))
print(a.issubset(range(N)), a.issuperset(list(range(N // 2))), a.isdisjoint(range(N, 2 * N)))
print(digest(a.intersection(range(N // 3, N))), digest(a.difference(list(range(N // 4)))))
print(digest(a.union(range(N, N + 5))), digest(a.symmetric_difference(range(N - 3, N + 3))))
z = set(a)
z.intersection_update(range(0, N, 3))
z.difference_update(range(0, N, 6))
z.symmetric_difference_update([1, 1, 2, N + 1])
z.update(range(N, N + 4))
print(digest(z), list(set([3, 1, 2, 3])), list(set(range(5)) | {9}))
# A slice of elements in, one only Python code finds: `_bulk` goes on.
w = set(range(N)) | {K(N + 5)}
print(digest(w & set(range(N + 10))), digest(set(range(N + 10)) - w), len(w | {N + 5}))
print(w == set(range(N)) | {N + 5}, w <= set(range(N + 10)))
# A copy without arguments, `{*s}` and `clear` (a new table).
u = a.union()
print(u == a, u is a, digest(a.difference()), digest({*a, *range(-3, 0)}), digest({*w}))
u.clear()
u.add(-1)
print(u, digest(a))
# A range is read in place, its items computed, not copied.
r = set(range(N, -N, -3))
print(digest(r), digest(frozenset(range(0))), len(dict.fromkeys(range(0, 3 * N, 2), 1)))
print(r.issubset(range(-N, N + 1)), digest(r & set(range(0, N, 2))), digest(r.symmetric_difference(range(-3, 3))))
print(sorted(set(range(2**62, 2**62 + 3))), digest(a.intersection(range(N // 2, 2 * N))))
del a, b, x, y, f, z, w

it = iter(range(2 * N))
d = dict(zip(it, it))
print(len(d), d[2 * N - 2], list(d.items())[:2])
e = dict(d)
e.update({-1: 0}, extra=1)
print(len(e), e[-1], e['extra'], e == d, d.copy() == d)
e = {0: 'x'}
e.update(d)
e |= [(-2, 'p'), (-3, 'q')]
print(len(e), e[0], e[-3], list(e)[:3])
print(len(d | {0: 'z'}), (d | {0: 'z'})[0], len(dict([(i, i) for i in range(N)])))
print(len(dict.fromkeys(range(N))), dict.fromkeys(range(N), 7)[N - 1], len(dict.fromkeys(set(range(N)))))
dd = {**d, **{0: 'y'}, -9: 1}
print(len(dd), dd[0], dd[-9], list(dd)[:2], list(dd)[-1])
mk = {K(1): 'k', **{i: -i for i in range(N)}}
print(len(mk), mk[1], type(list(mk)[0]).__name__, list(mk.values())[:3])
del dd, mk
k = d.keys()
print(len(k - {0}), len(k | {-1}), len(k ^ {0, -1}), len(k & range(0, 100, 2)), len(d.items() | {(0, 1)}))
# An items view read in place, its pairs as tuples.
di = dict(d.items())
print(di == d, list(di.items())[:2], len(set(d.items())), len(frozenset(d.items()) - {*d.items()}))
del di
mixed = [(i, i) for i in range(N)] + [(K(3), 'k'), (3, 'three')]
m = dict(mixed)
print(len(m), m[3], m[K(5)], type(list(m)[3]).__name__)
try:
    dict([(i, i) for i in range(N)] + [(1, 2, 3)])
except ValueError as ex:
    print('ValueError', ex)
del d, e, k, m, mixed

L = 300_000
xs = list(range(L))
t = tuple(xs)
print(len(xs + xs), (xs + xs)[L - 1:L + 1], len(t + t), type(t + t).__name__)
ys = list(xs)
zs = ys
ys += t
ys.extend(xs[:10])
print(zs is ys, len(ys), ys[2 * L - 1], ys[-1])
print(len(xs * 2), (xs * 2)[L + 3], len(t * 2), [0, 1, 2] * 3, digest([5] * L))
ys = list(range(L // 2))
ys *= 3
print(len(ys), ys[L // 2], ys[-1])
print(xs[::-1][:3], xs[7:L:5][:3], len(xs[::2]), t[::-3][:2], type(t[1:]).__name__)
ys = list(xs)
ys[10:L - 10] = [-1, -2]
print(ys)
ys = list(xs)
ys[::2] = range(L // 2)
print(ys[:6], ys[-3:])
ys = list(xs)
ys[::-1] = ys
print(ys[:3], ys[-3:])
ys = list(xs)
del ys[::3]
print(len(ys), ys[:4], ys[-2:])
ys = list(xs)
del ys[L - 1::-4]
print(len(ys), ys[:4], ys[-3:])
ys = list(xs)
del ys[0]
ys.insert(0, 'a')
ys.insert(L // 2, 'b')
print(ys.pop(1), ys.pop(L // 2), len(ys), ys[:3], ys[L // 2 - 1:L // 2 + 2])
ys.reverse()
print(ys[:3], len(ys.copy()), list(t)[-1], tuple(xs)[-2])
print(xs.count(5), xs.count(-1), t.count(L - 1), xs.index(L - 1), t.index(9, 5, 10))
try:
    xs.index(5, 6)
except ValueError as ex:
    print('ValueError', ex)
ys = list(xs)
ys.remove(L - 1)
ys.remove(0)
print(len(ys), ys[0], ys[-1])
ws = xs + [K(7), 7]
print(ws.count(7), ws.index(7), ws.index(K(L - 1)))
ws.remove(-3) if -3 in ws else ws.remove(7)
print(len(ws), ws[-1])

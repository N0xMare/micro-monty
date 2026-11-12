# Sorts long enough to merge in slices (more than 2^17 comparisons) park
# between slices: results stay stable, reverse keeps equal items in their
# order, keys and `list.sort` behave as one native sort did, and errors
# leave the list as CPython does.
import random

r = random.Random(423)
N = 20_000


def digest(xs):
    h = 0
    for x in xs:
        h = (h * 1_000_003 + len(repr(x))) % 1_000_000_007
    return h


ints = [r.randrange(-500, 500) for _ in range(N)]
strs = [str(r.randrange(10_000)) for _ in range(N)]
floats = [r.random() for _ in range(N)]
tups = [(r.randrange(20), str(r.randrange(20))) for _ in range(N)]
pairs = [(x, i) for i, x in enumerate(ints)]
for name, xs in [('ints', ints), ('strs', strs), ('floats', floats), ('tups', tups)]:
    for rev in (False, True):
        ys = sorted(xs, reverse=rev)
        zs = list(xs)
        zs.sort(reverse=rev)
        print(name, rev, ys == zs, ys[:3], ys[-3:], digest(ys))
for rev in (False, True):
    ys = sorted(pairs, key=lambda p: p[0], reverse=rev)
    print('stable', rev, [p[1] for p in ys[:6]], [p[1] for p in ys[-6:]])
    print('len', rev, sorted(strs, key=len, reverse=rev)[:6])
print('sorted input', sorted(range(N))[-2:], sorted(range(N, 0, -1))[:2])
print('by index', sorted(range(N), key=lambda i: strs[i])[:6])


class K:
    def __init__(self, v):
        self.v = v

    def __lt__(self, other):
        return self.v < other.v


print('user lt', [k.v for k in sorted(K(x) for x in ints)[:6]])
l = list(ints) + [None]
try:
    l.sort()
except TypeError as e:
    print(e, len(l), sorted(x for x in l if x is not None) == sorted(ints))
l = list(range(N, 0, -1))


def key(x):
    if x == 5:
        l.append(1)
    return x


try:
    l.sort(key=key)
except ValueError as e:
    print(e, len(l), l[:3])

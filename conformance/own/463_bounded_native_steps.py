# Bounded native steps (docs/spec.md §5): `in` over a long list or tuple
# goes on a step at a time in `_steps`, handing the rest to Python when an
# item's `==` is user code; long sorts fill, merge, reverse and map their
# arrays a slice per step. Results must equal one native run's.


class Loud:
    def __init__(self, v):
        self.v = v

    def __eq__(self, o):
        print('eq', self.v, o)
        return self.v == o


class Bad:
    def __eq__(self, o):
        raise KeyError('bad eq')


xs = list(range(400_000))
t = tuple(xs)
print(-1 in xs, 399_999 in xs, -1 not in t, 5 not in t, 200_000 in t)
ys = xs + [Loud(7), 7]
print(7.5 in ys, 'q' not in ys)
zs = xs + [Bad()]
try:
    print(-9 in zs)
except KeyError as e:
    print('KeyError', e)
strs = ['a' * 300 + str(i) for i in range(50_000)]
print(strs[-1] in strs, 'b' in strs)

ns = [(i * 7919) % 300_007 for i in range(300_001)]
for rev in (False, True):
    a = sorted(ns, reverse=rev)
    b = sorted(ns, key=lambda v: v // 7, reverse=rev)
    c = list(ns)
    c.sort(key=lambda v: -(v % 1000), reverse=rev)
    print(rev, a[:3], a[-3:], b[:3], b[-3:], c[:3], c[-3:])
pairs = [(v % 50, i) for i, v in enumerate(ns)]
print(sorted(pairs, key=lambda p: p[0], reverse=True)[:4])
print(sorted(tuple(ns), reverse=True)[149_999:150_002])
l = list(range(200_000))


def k(v):
    if v == 150_000:
        l.append(1)
    return -v


try:
    l.sort(key=k)
except ValueError as e:
    print(e, len(l), l[:3])

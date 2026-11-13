# A dict subclass's keys view no longer presizes a set as its dict does
# (CPython's `dictviews_to_set` presizes for an exact dict only), so the
# result keeps CPython's order. View operations with a long set subclass's
# instance or generator operand run in the prelude twin, in steps; the
# results are the same. Also: `d &= keys` and friends on a dict, `s & s`
# as a copy, `^=` on a set subclass in place, and the set comparison
# dunders as attributes.
from collections import Counter, OrderedDict, defaultdict


class DD(dict):
    pass


class SS(set):
    pass


class K:
    def __init__(self, v):
        self.v = v

    def __hash__(self):
        return hash(self.v)

    def __eq__(self, other):
        return isinstance(other, K) and other.v == self.v

    def __repr__(self):
        return f'K({self.v})'


keys = [64, 8, 72, 16, 24]
d = dict.fromkeys(keys)
for make in (DD, Counter, OrderedDict, lambda x: defaultdict(None, x)):
    m = make(d)
    print(type(m).__name__, list(m.keys() | d.keys()), list(m.keys() | []), list(m.keys() ^ []),
          list(m.keys() - []), list(m | d.keys()))
    u = m.keys()
    u |= d.keys()
    print(list(u))
print(list(d.keys() | d.keys()), list(set(DD(d).keys())))

# Long operands: the same results as the native operations.
N = 70_000
big = dict.fromkeys(range(N))
ss = SS(range(0, 2 * N, 2))
for name, f in (('keys - ss', lambda: big.keys() - ss), ('keys ^ ss', lambda: big.keys() ^ ss),
                ('keys & ss', lambda: big.keys() & ss), ('ss & keys', lambda: ss & big.keys()),
                ('ss - keys', lambda: ss - big.keys()), ('ss | keys', lambda: ss | big.keys()),
                ('items | ss', lambda: big.items() | ss), ('keys | gen', lambda: big.keys() | (i for i in range(-3, 3))),
                ('keys - gen', lambda: big.keys() - (i for i in range(N - 5))),
                ('DD | keys', lambda: DD(big) | {1: 2}.keys()), ('ss | s', lambda: ss | set(range(5))),
                ('ss - s', lambda: ss - set(range(N))), ('dd | d', lambda: DD(big) | {N: 0})):
    r = f()
    print(name, type(r).__name__, len(r), sum(hash(x) for x in r) % 1_000_003, list(r)[:3])
x = SS(range(N))
x |= set(range(N, N + 5))
print(type(x).__name__, len(x))

# A dict's in-place `&`, `-` and `^` with a view: a new set, as `d & keys`.
for op in ('&=', '-=', '^='):
    a = {1: 0, 2: 0, 3: 0}
    v = {2: 0, 5: 0}.keys()
    if op == '&=':
        a &= v
    elif op == '-=':
        a -= v
    else:
        a ^= v
    print(op, type(a).__name__, sorted(a))

s = set()
for k in (64, 8, 72, 16, 24, 40, 56):
    s.add(k)
s.discard(8)
s.add(128)
print(list(s), list(s & s), list(s.intersection(s)))
r = s
s &= s
print(list(s), s is r)
t = SS([K(1), K(2), 3])
r = t
t ^= {K(2), 4, 3}
print(type(t).__name__, t is r, sorted(map(repr, t)))
t = SS([K(1), 2])
t ^= t
print(type(t).__name__, len(t))

for name in ('__lt__', '__le__', '__gt__', '__ge__'):
    print(name, getattr(set, name)({1}, {1, 2}), getattr(frozenset, name)(frozenset([1, 2]), {1}),
          getattr({1}, name)([1]))

# Every in-place operator with keys hashed in Python: a set or a set
# subclass's instance changes in place; a frozenset gives a new one.
for left in (set, SS, frozenset):
    for op in ('|=', '&=', '-=', '^='):
        for right in (lambda: SS([K(0), 5, 0]), lambda: {K(0), 5, 0}, lambda: frozenset([K(0), 5])):
            s = left([0, 64, K(0)])
            old = s
            x = right()
            if op == '|=':
                s |= x
            elif op == '&=':
                s &= x
            elif op == '-=':
                s -= x
            else:
                s ^= x
            print(left.__name__, op, type(x).__name__, type(s).__name__, s is old, len(s), len(old))

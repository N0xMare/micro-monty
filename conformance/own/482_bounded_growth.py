# Bounded native steps for a growing dict or set (docs/spec.md §5): once a
# full table holds `set::GROW_STEPS` (262,144) live entries, the insert
# that needs room (`d[k] = v`, a comprehension, `dict.__setitem__`,
# `setdefault`, `set.add`) hands the growth to `_steps` (`BULK`): the new
# tables a step each, the old entries a slice per step, then the item. A
# key hashed in Python takes the prelude twin, which grows in one step.
# Order, values and results must be CPython's.

P = 1_000_003
N = 263_000


class K:
    # Hashed and compared in Python.
    def __init__(self, v):
        self.v = v

    def __hash__(self):
        return hash(self.v)

    def __eq__(self, o):
        return isinstance(o, K) and o.v == self.v

    def __repr__(self):
        return f'K({self.v})'


def show(c):
    xs = list(c)
    return len(xs), xs[:4], xs[-4:], sum(x % 97 if type(x) is int else len(x) for x in xs)


# `d[k] = v` with deleted entries in the table (fewer live entries than
# 262,144 when it is full, so it grows in one step, compacted).
d = {}
for i in range(N):
    d[i * 7 % P] = i
    if i % 40_000 == 0:
        del d[i // 2 * 7 % P]
print(show(d), sum(d.values()) % P)
del d

# A comprehension (str keys), and `dict.__setitem__`.
e = {str(i): i for i in range(N)}
f = {}
for i in range(N):
    dict.__setitem__(f, -i, i)
print(show(e), show(f), f[-(N - 1)])
del e, f

# `setdefault`: its value is the stored one, also across the growth.
g = {}
got = 0
for i in range(N):
    got += g.setdefault(i % (N - 5), i)
print(show(g), got, g.setdefault(3, 'x'))
del g

# Tables full at 262,144 live entries: the next int grows in steps, a key
# hashed in Python through the twin; then deletions and more inserts.
h1 = {}
h2 = {}
for i in range(262_144):
    h1[i] = i
    h2[i] = i
h1[-1] = 1
h2[K(-1)] = 1
for i in range(0, 262_144, 1_000):
    del h1[i]
    del h2[i]
for i in range(500):
    h1[-i - 2] = i
    h2[-i - 2] = i
print(show(h1), h1[-1], len(h2), h2[K(-1)], list(h2)[-500:-498])
del h1, h2

# `set.add` and a set comprehension past 314,572 elements (a table of
# 2^19 slots 60% full): iteration order is the rebuilt table's.
s = set()
for i in range(316_000):
    s.add(i * 7 % P)
    if i % 50_000 == 0:
        s.discard(i // 2 * 7 % P)
print(show(s), (P - 7) in s)
del s
t = {i * 13 % P for i in range(316_000)}
print(show(t), (P - 13) in t)

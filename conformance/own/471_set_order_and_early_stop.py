# Set iteration order after deletions, and the set methods that stop
# reading their argument early, as CPython 3.14 does.
#
# - `add` reuses the last dummy slot its probe passed (`set_add_entry`).
# - `difference_update` and `-=` rebuild a table more than a quarter
#   dummies when they end; `intersection_update` and `&=` take the new
#   table whole (`set_swap_bodies`), keeping the finger.
# - `intersection` over a non-set stops once it holds all of the set,
#   and `issubset` with it; `issuperset` stops at the first missing item;
#   `isdisjoint` at the first common one. Items past that point are not
#   read, so an unhashable one raises nothing and a generator is left
#   where it stopped.


def g(log, xs):
    for x in xs:
        log.append(x)
        yield x


def t(f):
    try:
        print(f())
    except Exception as e:
        print(type(e).__name__, e)


t(lambda: {1}.issubset([1, [2]]))
t(lambda: {1}.intersection([1, [2]]))
t(lambda: {1}.issuperset([2, [3]]))
t(lambda: {1, 2}.intersection([1, [2]]))
t(lambda: {1}.issuperset([1, [3]]))
t(lambda: {1}.isdisjoint([1, [3]]))
s = {1}
t(lambda: s.intersection_update([1, [2]]))
print(s)
t(lambda: frozenset({1}).intersection([1, [2]]))
t(lambda: {1}.intersection([1], [1, [2]]))
t(lambda: set().intersection(g([], [[1]])))
for name in ['issubset', 'intersection', 'issuperset', 'isdisjoint', 'intersection_update']:
    log = []
    s = {0, 1}
    t(lambda: getattr(s, name)(g(log, range(5))))
    print(name, log, s)
d = {1: 'a', 2: 'b'}
print({1, 2}.intersection(d.keys()), {(1, 'a')}.issubset(d.items()), {1}.issuperset(d.keys()))
print({1, 2, 3}.intersection(range(1, 10**9)), {1, 2}.issubset(range(10**12)))


class K:
    # Equal to the int it wraps, so only Python code can find it.
    def __init__(self, v):
        self.v = v

    def __hash__(self):
        return hash(self.v)

    def __eq__(self, o):
        return getattr(o, 'v', o) == self.v


print(sorted(x.v for x in {1, 2}.intersection([K(1), K(2), [3]])))
t(lambda: {1, 2}.issubset([K(2), K(1), [3]]))
t(lambda: {1, 2}.issuperset([K(3), [3]]))

# Order after pops, discards and re-adds.
s = set(range(20))
out = [s.pop() for _ in range(5)]
s.add(0)
s.add(1)
s.add(100)
out += [s.pop() for _ in range(4)]
print(out, list(s))
s = set(range(100))
s -= set(range(10, 100))
s.add(1000)
s.add(64)
print(list(s))
s = set(range(100))
s.difference_update(g([], range(3, 100)))
s.add(64)
print(list(s))
s = set(range(1000))
for _ in range(990):
    s.pop()
s |= set(range(500, 520))
print(list(s)[:5], [s.pop() for _ in range(8)])
s = set(range(200))
s.intersection_update([7, 3, 150, 64])
s.add(1)
print(list(s), s.pop())

# A seeded mix of every set operation, its order compared whole.
x = 987654


def rnd(n):
    global x
    x = (x * 1103515245 + 12345) % 2147483648
    return (x >> 8) % n


def items(k, m):
    return [rnd(m) * (1 if rnd(3) else 7919) - 20 for _ in range(k)]


def arg():
    k = rnd(6)
    xs = items(rnd(40), 400)
    if k == 0:
        return set(xs)
    if k == 1:
        return xs
    if k == 2:
        return dict.fromkeys(xs)
    if k == 3:
        return frozenset(xs)
    if k == 4:
        return (y for y in xs)
    return tuple(xs)


h = 0
for trial in range(60):
    s = set(items(rnd(300), 600))
    for step in range(rnd(40) + 1):
        op = rnd(17)
        o = arg()
        if op == 0:
            s |= set(o)
        elif op == 1:
            s -= set(o)
        elif op == 2:
            s &= set(o)
        elif op == 3:
            s ^= set(o)
        elif op == 4:
            s.update(o)
        elif op == 5:
            s.difference_update(o)
        elif op == 6:
            s.intersection_update(o)
        elif op == 7:
            s.symmetric_difference_update(o)
        elif op == 8:
            s = s - set(o)
        elif op == 9:
            s = s & set(o)
        elif op == 10:
            s = s.difference(o)
        elif op == 11:
            s = s.intersection(o)
        elif op == 12:
            s = s.union(o)
        elif op == 13:
            for _ in range(rnd(20)):
                if s:
                    s.pop()
        elif op == 14:
            for y in items(rnd(30), 600):
                s.discard(y)
        elif op == 15:
            for y in items(rnd(30), 600):
                s.add(y)
        else:
            h = hash((h, s.issubset(o), s.issuperset(arg()), s.isdisjoint(arg())))
        h = hash((h, tuple(s)))
    if trial % 10 == 9:
        print(trial, h)

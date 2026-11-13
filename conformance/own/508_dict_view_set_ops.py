# Dict-view set operations keep CPython's order and results
# (dictobject.c): `dictviews_to_set` presizes for a keys view's dict, an
# items view's pairs are added one by one, and a set built from a keys
# view adds its keys one by one (no presize); `&` reads the other operand
# unless it is an exact set at least as long as the view, of two views
# the shorter; `items ^ items` compares values; a view is ordered against
# a set subclass's instance; `isdisjoint` probes the longer operand.
class SS(set):
    pass


d = dict.fromkeys([64, 8, 72, 16, 24])
print(list(set(d.keys())), list(set(d)), list({*d.keys()}), list(frozenset(d.keys())), list(SS(d.keys())))
s = set()
s |= d.keys()
print(list(s), list(set().union([], d.keys())), list(set(d.items()))[:2])
print(list({0, 8, 16} | dict.fromkeys([16, 8, 0]).keys()))
e = dict.fromkeys([0, 64, 128, 8, 72], 1)
f = dict.fromkeys([5, 13, 21, 29, 37, 45, 53], 2)
for x, y in ((e.keys(), f.keys()), (f.keys(), e.keys()), (e.items(), f.keys()), (e.keys(), {5, 0}),
             ({5, 0}, e.keys()), (e.keys(), SS([5, 0, 64])), (SS([5, 0, 64]), e.keys()), (e.keys(), [5, 64, 0]),
             ([5, 64, 0], e.keys()), (e.items(), {(0, 1), 3}), (e.keys(), f)):
    print(list(x | y), list(x & y), list(x - y), list(x ^ y))
print(list(e.keys() & dict.fromkeys([128, 0, 64, 8, 72, 99]).keys()), list({0: 1, 8: 2}.items() & e.items()))
print(list({1: 2, 3: 4}.items() ^ {1: 2, 3: 5, 6: 7}.items()))
print({1: 0}.keys() <= SS([1, 2]), SS([1, 2]) >= {1: 0}.keys(), {1: 0}.items() < SS([(1, 0), 2]),
      {1: 0}.keys() > SS(), {1: 0}.keys() == SS([1]), {1: 0}.keys() != SS([1]))
print(e.keys().isdisjoint(f.keys()), e.keys().isdisjoint([9, 0]), e.items().isdisjoint({(0, 1)}),
      e.keys().isdisjoint(e.keys()), {}.keys().isdisjoint({}.keys()))
for g in (lambda: e.keys() | 5, lambda: e.keys() - [[]], lambda: e.keys() & [[]], lambda: {1: []}.items() ^ {1: []}.items()):
    try:
        g()
    except TypeError as err:
        print('TypeError', err)


class K:
    def __init__(self, v):
        self.v = v

    def __hash__(self):
        return hash(self.v)

    def __eq__(self, o):
        return isinstance(o, K) and o.v == self.v

    def __repr__(self):
        return f'K({self.v})'


# Keys hashed in Python take the prelude twins, in the same order.
kd = dict.fromkeys([K(x) for x in (0, 64, 128, 8, 72)], 1)
ks = {K(x) for x in (0, 64, 5, 128, 24)}
print(list(set(kd.keys())), list(kd.keys() | ks), list(kd.items() | ks), list(ks ^ kd.items()))
print(list(kd.keys() | SS(ks)), list(kd.keys() - SS(ks)), list(SS(ks) & kd.keys()), list({K(0)}.union(SS(ks))))
print(kd.keys() <= SS(ks), kd.keys().isdisjoint([K(5)]), kd.items().isdisjoint({(K(0), 1)}))
# Big operands: the same orders through the stepped paths.
n = 100_000
big = dict.fromkeys(range(0, n, 3))
other = set(range(0, n, 2))
for r in (big.keys() | other, other | big.keys(), big.keys() & other, big.keys() ^ other, set(big.keys()),
          big.items() - {(0, None)}):
    print(len(r), hash(tuple(r)))
# A union with the set itself is a copy, as in CPython's `set_or` and
# `set_union`, so it presizes nothing more; a second view adds its keys
# one by one, unpresized.
u = set(range(0, 3 * n, 7))
for r in (u | u, u.union(u), u.union(u, [1]), d.keys() | d.keys(), big.keys() | big.keys()):
    print(len(r), hash(tuple(r)))

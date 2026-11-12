# Set results keep CPython's table order (setobject.c): `a & b` runs a
# rebuild its last add started before it stops early; `a ^ b` copies the
# right operand and toggles the left one's elements in; a `union` or
# `difference` of several arguments copies once and updates in place; a
# frozenset built from an iterator, or by a frozenset method, keeps the
# table it was built in.
a = {8, 1, 2, 3, 4}
b = {8, 1, 2, 3, 4}
print(list(a & b), list(a.intersection(b)), list(frozenset(a) & b))
print(list({1} ^ {9}), list({1}.symmetric_difference({9})), list({1}.symmetric_difference([9, 9])))
print(list(frozenset({1}) ^ {9}), list({1, 2}.symmetric_difference({9: 0, 1: 0})))
print(list({1, 2, 3, 4}.union([8], [16])), list({1, 2, 3, 4}.union({8}, (16,), [32])))
print(list({1, 2, 3, 4, 8, 16, 32}.difference([1], {2}, (3,))), list(frozenset({1, 2, 3, 4}).union([8], [16])))
xs = [i * 524288 % 999983 for i in range(20)]
print(list(frozenset(x for x in xs)), list(frozenset(map(int, xs))) == list(frozenset(x for x in xs)))
s = {i * 524288 % 999983 for i in range(40)}
for o in ([7, 8], {7: 0, 8: 0}, (y for y in [7, 8])):
    print(list(s ^ set([7, 8])) == list(s.symmetric_difference(o)), list(frozenset(s).union(o if type(o) is not type(x for x in ()) else [7, 8]))[:5])


class K:
    def __init__(self, v):
        self.v = v

    def __hash__(self):
        return self.v

    def __eq__(self, o):
        return isinstance(o, K) and o.v == self.v

    def __repr__(self):
        return f'K({self.v})'


# Elements hashed in Python take the prelude twins, in the same order.
ks = [K(i * 524288 % 999983) for i in range(12)]
print(list(frozenset(k for k in ks)), list({K(1)} ^ {K(9)}), list({K(1), K(2)}.symmetric_difference([K(9), K(1)])))
print(list(frozenset({K(1)}).union([K(8)], [K(16)])), list(frozenset(ks).difference([ks[0]], [ks[1]]))[:3])
print(type(frozenset(ks).symmetric_difference([K(3)])).__name__, type(frozenset(ks).intersection()).__name__)
t = set(ks)
t.symmetric_difference_update({K(1), ks[0]})
print(list(t)[:4], len(t))
# Big operands: the same orders through the stepped paths.
n = 200_000
p = set(range(0, n, 3))
q = set(range(0, n, 2))
for r in (p & q, q & p, p ^ q, q ^ p, p.union(range(5), [n + 1], {n + 2}), p.difference(range(9), [12], q)):
    print(len(r), hash(tuple(r)))
c = set(range(1000))
print(hash(tuple(c & set(range(1000)))), hash(tuple(set(range(1000)) & c)))
# The prelude twins (an argument that is a generator) build the same
# tables: `update` with a set or dict presizes as `set_merge` does, and
# `difference` takes its first argument as `set_difference` does.
xs = [55, 340, 128, 1615456, 863151]
a1 = [58, 483039, 202, 269, 16, 236, 71, 1575861, 29, -18, -4, 88, 289, 366, 847313, 332, 633500, 15818, 2898334, 16]
a2 = set(i * 7919 % 3001 for i in range(40))
s = set().symmetric_difference(y for y in xs)
print(list(frozenset(s).union((y for y in a1), a2)), list(set(s).union(a1, (y for y in a2)))[:8])
big = set(i * 524288 % 999983 for i in range(53))
print(list(big.difference(frozenset(list(big)[:15]), (y for y in a1), tuple(list(big)[20:30]))))
print(list(frozenset(big).difference({y: 0 for y in list(big)[::3]}, (y for y in ()))))
t = set(big)
t.symmetric_difference_update(dict.fromkeys([7, 8] + list(big)[:4]))
print(list(t)[:10])
# A key that cannot be hashed in the prelude twins gets CPython 3.14's
# message, naming the key's type and the item's error, with no context.
H = type('H', (), {'__hash__': lambda s: 1, '__eq__': lambda s, o: False})
U = type('U', (), {'__hash__': lambda s: (_ for _ in ()).throw(TypeError('mine'))})
for f in (lambda: set(zip([H(), [5]], [4, 4])), lambda: dict.fromkeys(x for x in [H(), ([1], 2)]),
          lambda: frozenset(x for x in [H(), (U(),)]), lambda: {H(): 0, U(): 1}, lambda: set(x for x in [H(), {}])):
    try:
        f()
    except TypeError as e:
        print(e, e.__context__)

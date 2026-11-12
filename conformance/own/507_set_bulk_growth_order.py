# A set built in bulk keeps CPython's table order when the add that fills
# it past 3/5 comes late in a step (past about 32k items): that item stays
# in the old table and moves with the others in slot order, as
# `set_add_entry` leaves it, instead of being added again last.
import gc
import random

random.seed(12)
l = [random.getrandbits(40) for _ in range(80000)]
a = set()
for v in l:
    a.add(v)
order = list(a)


def same(s):
    return list(s) == order


b = set(l)
print(a == b, same(b), same(set(iter(l))), same(set(tuple(l))), same(frozenset(l)))
u = set()
u.update(l)
print(same(u), same(set(b)), same({v for v in l}), same(set(v for v in l)))
print([order.index(x) for x in (708020607913, 527225396137)])

N = 314571
A, B = 524280 + 2 * (1 << 20), 524280 + (1 << 20)
m = list(range(100000, 100000 + N)) + [A, B]
d = dict.fromkeys(m)


def at(s):
    o = list(s)
    return len(o), o.index(A), o.index(B)


s = set(m)
print(at(s), at(frozenset(m)), at(set(s)))
# Under `cargo xtask gc-stress --incremental` a mark is always in
# progress, so the line above's temporaries stay until it ends, and
# `frozenset(m)` below, which is not rerun after a collection, would meet
# the 256 MiB limit (HANDOFF 0.0.2, near-limit retry coverage).
gc.collect()
print(at(d.keys() & s), at(s & d.keys()), at(s - {1}), at(d.keys() & frozenset(m)))

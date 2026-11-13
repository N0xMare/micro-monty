# Big ints against floats at the edges: infinities, nan, -0.0, the floats
# just below and past 2^1023, and ints either side of 2^1024 (past every
# finite float, an int compares with a float by its sign alone), with
# `<`, `==`, `>`, `hash`, and the folds and sorts that compare them.
import sys

inf = float('inf')
nan = float('nan')
fmax = sys.float_info.max
floats = [
    inf, -inf, nan, -0.0, 0.0, 1.0, -1.0,
    fmax, -fmax, 2.0**1023, -(2.0**1023), 1.5 * 2.0**1023, -1.5 * 2.0**1023,
    2.0**1022, 2.0**63, -(2.0**63), 2.0**64, 9007199254740993.0, 1e308,
]
ints = [
    0, 1, -1, 2**63, -(2**63), 2**64, 2**1022, 2**1023, -(2**1023),
    2**1023 + 1, 3 * 2**1022, int(fmax), int(fmax) + 1, -int(fmax), -int(fmax) - 1,
    2**1024 - 1, -(2**1024 - 1), 2**1024, -(2**1024), 2**1024 + 1, 2**2000, -(2**2000),
    (1 << 100000) | 5, -((1 << 100000) | 5),
]
for i in ints:
    row = []
    for f in floats:
        row.append('%d%d%d%d%d' % (i < f, i == f, i > f, f < i, i != f))
    print(i.bit_length(), i < 0, ' '.join(row))

# hash agrees with == across int and float
for f in floats:
    if f == f and f not in (inf, -inf):
        print(repr(f), hash(f) == hash(int(f)), int(f) == f)
print(hash(inf), hash(-inf), hash(-0.0), hash(0.0))
for i in ints:
    print(i.bit_length(), hash(i), hash(-i), hash((i, -i, 1.5)))

# folds, scans and sorts over ints and floats past 2^1023
mixed = [2**1024, 1.5 * 2.0**1023, -inf, fmax, 2**1023, -(2**2000), inf, 2.0**1023, 2**1023 + 1, -0.0, 0]
print([(type(x).__name__, x.bit_length() if isinstance(x, int) else x) for x in sorted(mixed)])
print(max(mixed) == inf, min(mixed) == -inf, max(x for x in mixed if x != inf) == 2**1024)
print(mixed.count(2.0**1023), 2.0**1023 in [2**1023], (2**1023) in [2.0**1023], 2**1024 in [inf, fmax])
print([2**1023, 2.0**1023].index(2.0**1023), (2**1023, 1) == (2.0**1023, 1), (2**1024, 0) < (inf, 0))
big = (1 << 100000) | 5
print(sorted([big, -big, big + 0, 2**1024, -inf])[3:] == [big, big + 0])
print({big: 1}.get(big + 0), {2**1023: 'i'}.get(2.0**1023), len({2**1023, 2.0**1023, 2**1024}))
print(hash(frozenset([big, big + 0, 2**1023, 2.0**1023])) == hash(frozenset([big, 2**1023])))

# Membership, counts and folds of big ints against nan and the infinities
# (decided before the int is read).
zs = [big, -big, 2**1024, -(2**1024), 2**1023, 0]
for f in (inf, -inf, nan, -0.0, fmax, -fmax):
    print(repr(f), f in zs, zs.count(f), [z < f for z in zs], [z == f for z in zs], [f >= z for z in zs])
print(max(zs + [-inf]) == big, min(zs + [inf]) == -big, sorted(zs + [inf, -inf])[::7] == [-inf, inf])
print(zs.index(-(2**1024)), (inf, big) > (inf, 0), [big, nan].count(nan), big in [nan, -inf, inf])

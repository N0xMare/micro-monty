# `x in d.values()` over a long dict goes through the entries in place, a
# step of bounded work at a time (`nstep::op::DV_IN`): deleted entries are
# passed over, a value whose `==` needs Python code hands the whole test to
# the prelude (which compares the earlier values again, without Python
# code), and `not in` is its negation.

N = 400_000
d = {i: i * 2 for i in range(N)}
for k in range(0, N, 3):
    del d[k]
print(len(d))
print(-1 in d.values(), -1 not in d.values())
print(14 in d.values(), 6 in d.values(), 6 not in d.values())
print((N - 2) * 2 in d.values(), (N - 1) * 2 in d.values())

# Equal but not the same object, and the same object.
big = 10**30
d[-1] = big
print(10**30 in d.values(), big in d.values())
s = {i: str(i) for i in range(N)}
print(str(N - 1) in s.values(), str(N) in s.values(), N - 1 in s.values())


class Eq:
    # Equal to one value; counts its comparisons.
    def __init__(self, to):
        self.to = to
        self.n = 0

    def __eq__(self, other):
        self.n += 1
        return other == self.to


e = Eq((N - 1) * 2)
print(e in d.values(), e.n > 0)
e = Eq(-7)
print(e not in d.values(), e.n == len(d))


class Late:
    # A value far into the dict whose `==` runs Python code.
    def __init__(self):
        self.seen = []

    def __eq__(self, other):
        self.seen.append(other)
        return other == 'late'


late = Late()
d[N + 1] = late
d[N + 2] = 'late'
print('late' in d.values(), late.seen)
print('never' in d.values(), late.seen)
print(late in d.values())

print(1 in {}.values(), 1 not in {}.values(), 1 in {1: 1}.values())
v = d.values()
del d[N + 1], d[N + 2], d[-1]
print(big in v, 'late' in v, len(v))

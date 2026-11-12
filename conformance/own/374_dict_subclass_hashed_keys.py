# Subscripts on dict subclasses with keys whose __hash__ is Python code:
# the prelude twin must not hand the subscript back to itself.
import collections
from fractions import Fraction


class K:
    def __init__(self, n):
        self.n = n

    def __hash__(self):
        return self.n % 3

    def __eq__(self, other):
        return isinstance(other, K) and other.n == self.n

    def __repr__(self):
        return f'K({self.n})'


class D(dict):
    pass


class M(dict):
    def __missing__(self, k):
        return ('missing', k)


d = D()
for i in range(6):
    d[K(i)] = i
print(len(d), d[K(4)], K(4) in d)
del d[K(4)]
print(len(d), K(4) in d)
try:
    d[K(4)]
except KeyError as e:
    print('KeyError', e)
try:
    del d[K(4)]
except KeyError as e:
    print('KeyError', e)
m = M()
m[K(1)] = 'one'
print(m[K(1)], m[K(2)])
c = collections.Counter([Fraction(1, 3), Fraction(1, 3), Fraction(1, 2)])
print(c[Fraction(1, 3)], c[Fraction(2, 3)], sorted(c.items()))
dd = collections.defaultdict(list)
dd[K(7)].append(1)
dd[K(7)].append(2)
print(dict(dd))

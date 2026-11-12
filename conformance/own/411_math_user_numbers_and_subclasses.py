# math.prod / math.sumprod run the operators of user numbers (Fraction,
# Decimal, also through statistics.geometric_mean and linear_regression),
# which raised "micro-monty does not support user-defined objects";
# math.dist takes what __float__ gives.
import math, statistics
from fractions import Fraction as F
from decimal import Decimal as D
print(math.prod([1., F(3, 2)]), math.prod([F(1, 2), F(2, 3)]), math.prod(iter([F(1, 2), 3])), math.prod((D('1.5'), 2), start=D(2)))
print(math.prod(x for x in [F(1, 3), 3]), math.prod([2, 3], start=F(1, 7)))
print(math.sumprod([F(1, 2), 2], [3, F(1, 3)]), math.sumprod([D('0.1'), D('0.2')], [1, 2]), math.sumprod([1, 2.5], [3, 4]))
try: math.sumprod([F(1)], [1, 2])
except ValueError as e: print(e)
class Bad:
    def __mul__(self, o): raise RuntimeError('mul')
    __rmul__ = __mul__
for f in (lambda: math.prod([2, Bad()]), lambda: math.sumprod([1, Bad()], [1, 2])):
    try: f()
    except RuntimeError as e: print('RuntimeError', e)
class FloatLike:
    def __init__(self, v): self.v = v
    def __float__(self): return self.v
print(math.dist((FloatLike(14.), 1), (2, -4)), math.dist([F(3), 0], [0, F(4)]))
print(statistics.geometric_mean([F(1), F(4)]), statistics.linear_regression([F(1), F(2), F(3)], [F(2), F(4), F(7)]))
print(statistics.correlation([F(1), F(2), F(3)], [F(2), F(4), F(7)]))
try: math.prod([1, 'a', 2])
except TypeError as e: print(e)
print(math.prod(['ab', 3]))

# int and float subclasses without their own __index__/__float__ are their
# value: comb/perm/isqrt took only exact ints, isinf/floor only floats.
import enum
class I(int): pass
class Fl(float): pass
class Size(enum.IntEnum):
    N = 6
print(math.comb(I(5), I(2)), math.perm(I(5), 2), math.isqrt(I(17)), math.factorial(I(4)),
      math.gcd(I(4), 6), math.comb(Size.N, 2), math.isqrt(Size.N))
x = Fl(2.5)
print(math.isinf(x), math.floor(x), math.sqrt(Fl(6.25)), math.isnan(Fl('nan')), math.isfinite(x),
      math.log2(I(8)), math.exp(Fl(0)))

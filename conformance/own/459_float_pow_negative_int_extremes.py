# x ** -n for floats whose |x| ** n is subnormal or near the bottom of the
# normal range: the result overflows (OverflowError, never nan) or is large
# and within 1 ulp of the exact value. Found by difffuzz (seed 20223999).
import math
from fractions import Fraction


def show(f):
    try:
        print(repr(f()))
    except Exception as e:
        print(type(e).__name__, e)


show(lambda: ((+-2.25 / abs(1e300)) / -1e10) ** -1)
show(lambda: 2.25e-310 ** -1)
show(lambda: (-2.25e-310) ** -1)
show(lambda: 1e-160 ** -2)
show(lambda: (-1e-155) ** -3)
show(lambda: pow(5e-324, -1))
show(lambda: math.pow(2.25e-310, -1.0))
show(lambda: math.pow(1e-155, -2.0))
print(2.5e-308 ** -1 == 1 / 2.5e-308, 1e-150 ** -2 > 1e299)

# |x| ** n from 2^-1100 to 2^-940, n up to 31: count results that are nan,
# overflow when the exact value does not, or are more than 1 ulp off.
x = 2025
bad = []
overflows = 0
for i in range(3000):
    x = (x * 6364136223846793005 + 1442695040888963407) % 2**64
    n = (1, 2, 3, 5, 7, 31)[x % 6]
    e = -1100 + (x >> 8) % 160
    b = math.ldexp(1 + (x >> 12) % 2**40 / 2**40, e // n)
    if b == 0.0:
        continue
    exact = Fraction(1) / Fraction(b) ** n
    try:
        float(exact)
        big = False
    except OverflowError:
        big = True
    try:
        r = b ** -n
    except OverflowError:
        overflows += 1
        if not big:
            bad.append((b.hex(), n, "overflow"))
        continue
    if big or r != r or abs(Fraction(r) - exact) > Fraction(math.ulp(r)):
        bad.append((b.hex(), n, repr(r)))
print(overflows > 100, bad)

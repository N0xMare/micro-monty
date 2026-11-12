# math.pow and ** at infinities, zeros and signs; logarithms of subnormals
# and of values next to 1; cmath at huge arguments; complex products that
# overflow or hold an infinity.
import math, cmath

inf, nan = math.inf, math.nan
print(math.pow(2, inf), math.pow(0.5, inf), math.pow(1, inf), math.pow(-1, inf), math.pow(2, -inf), math.pow(0.5, -inf))
print(math.pow(0.0, -inf), math.pow(-0.5, inf), math.pow(-2, inf), math.pow(-2, -inf), math.pow(nan, 0), math.pow(1, nan))
print(math.pow(inf, 3), math.pow(-inf, 3), math.pow(-inf, 2), math.pow(inf, -3), math.pow(-inf, -3), math.pow(-inf, -2))
for f in (lambda: math.pow(10.0, 400), lambda: math.pow(0.0, -1), lambda: math.pow(-8, 1 / 3)):
    try:
        print(f())
    except (OverflowError, ValueError) as e:
        print(type(e).__name__, e)
print(0.0 ** 3, (-0.0) ** 3, (-0.0) ** 2, inf ** 2, (-inf) ** 3, (-inf) ** -3, (-inf) ** 0.5)
for f in (lambda: 0.0 ** -1, lambda: (-0.0) ** -3, lambda: (-0.0) ** -2):
    try:
        print(f())
    except ZeroDivisionError as e:
        print(e)
print(math.log(5e-324), math.log(1e-310), math.log2(1e-310), math.log10(5e-324), math.log1p(1e-310),
      math.log1p(-0.5), math.log1p(1e-17))
print(math.log(1.0000000000000002), math.log(0.9999999999999999), math.log1p(2**-30), math.log1p(-2**-30))
print(cmath.asinh(1e300+1e300j), cmath.asinh(1e300-1e300j), cmath.atanh(1e300+1j), cmath.atanh(1+1e-200j))
for z in (-800+1j, -800-1j, -800+0.5j, 800+0j):
    for f in (cmath.sinh, cmath.cosh):
        try:
            print(f(z))
        except OverflowError as e:
            print('OverflowError', e)
print(complex(1e308, 1e308) * complex(1e308, 1e308), complex(inf, 1) * complex(1, 1), complex(nan, inf) * 2j)
print(cmath.polar(3+4j))
try:
    cmath.polar(complex(1.7e308, 1.7e308))
except OverflowError as e:
    print('OverflowError', e)
for f in (lambda: 1.5 + 'a', lambda: 1.5 @ 2.0, lambda: 1.5 | 1, lambda: 1.5 << 1, lambda: divmod(1.5, 'x'),
          lambda: 1.5 ** 'x'):
    try:
        print(f())
    except TypeError as e:
        print(e)
print((5).is_integer(), True.is_integer(), (1.5).conjugate(), (2.0).conjugate(), (1+2j).__complex__(),
      complex.__complex__(3j))

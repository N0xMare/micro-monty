# Number methods, pow and log special cases, float operator errors, an
# __init__ that is a property, and canonical reordering in NFD/NFC.
print((5).is_integer(), True.is_integer(), (1.5).conjugate(), (2.0).conjugate(), (1+2j).__complex__(), complex.__complex__(3j))
import math, cmath
inf, nan = math.inf, math.nan
pass
print(math.pow(inf, 3), math.pow(-inf, 3), math.pow(-inf, 2), math.pow(inf, -3), math.pow(-inf, -3), math.pow(-inf, -2))
print(0.0 ** 3, (-0.0) ** 3, (-0.0) ** 2, inf ** 2, (-inf) ** 3, (-inf) ** -3, (-inf) ** 0.5)
for f in (lambda: 0.0 ** -1, lambda: (-0.0) ** -3, lambda: (-0.0) ** -2):
    try: print(f())
    except ZeroDivisionError as e: print(e)
print(math.log(5e-324), math.log(1e-310), math.log2(1e-310), math.log10(5e-324), math.log1p(1e-310), math.log1p(-0.5), math.log1p(1e-17))
print(math.log(1.0000000000000002), math.log(0.9999999999999999), math.log1p(2**-30), math.log1p(-2**-30))
print(cmath.asinh(1e300+1e300j), cmath.asinh(1e300-1e300j), cmath.atanh(1e300+1j), cmath.atanh(1+1e-200j))
for f, z in ((cmath.sinh, -800+1j), (cmath.cosh, -800+1j), (cmath.sinh, -709.5-1j), (cmath.cosh, -709.5+0.5j)):
    try:
        print(f(z))
    except OverflowError as e:
        print(e)
print(complex(1e308, 1e308) * complex(1e308, 1e308), complex(inf, 1) * complex(1, 1), complex(nan, inf) * 2j)
print(cmath.polar(complex(1e308, 1e308)) if False else cmath.polar(3+4j))
try:
    cmath.polar(complex(1.7e308, 1.7e308))
except OverflowError as e:
    print('OverflowError', e)
for f in (lambda: 1.5 + 'a', lambda: 1.5 @ 2.0, lambda: 1.5 | 1, lambda: 1.5 << 1, lambda: divmod(1.5, 'x'), lambda: 1.5 ** 'x'):
    try: print(f())
    except TypeError as e: print(e)
class A:
    __init__ = property(lambda s: None)
try:
    A()
except TypeError as e:
    print('TypeError')
import unicodedata as u
print([hex(ord(c)) for c in u.normalize('NFD', 'a\u0301\u0328')], [hex(ord(c)) for c in u.normalize('NFC', 'a\u0301\u0328')])

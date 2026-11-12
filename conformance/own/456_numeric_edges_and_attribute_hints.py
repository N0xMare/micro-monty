# Large and non-finite cmath and complex products, logs of subnormals,
# log1p near zero, OSError.__str__, exception __cause__, throw(),
# dir() of a wide instance, a long regex repetition, print(**5) and
# attribute hints per builtin type.
import cmath, math, re
print(cmath.asinh(complex(1e300, 1e300)), cmath.asinh(complex(1e300, -1e300)), cmath.asinh(complex(-1e300, -1e300)))
for fn, z in ((cmath.cosh, complex(709.5, 1)), (cmath.sinh, complex(-709.5, 1)), (cmath.cosh, complex(-709.5, -1)), (cmath.sinh, complex(709.5, -2)), (cmath.cosh, complex(800, 1))):
    try:
        print(fn(z))
    except OverflowError as e:
        print(e)
inf, nan = float('inf'), float('nan')
for a, b in ((complex(inf, nan), complex(1, 1)), (complex(nan, inf), complex(0, 1)), (complex(inf, 1), complex(nan, 0)), (complex(1, 1), complex(inf, nan))):
    print(a * b)
print(math.log(5e-324), math.log2(5e-324), math.log10(5e-324), math.log(2.2250738585072014e-308 / 3))
print([math.log1p(x) for x in (1e-5, 0.2, -0.2, 0.4, 0.41, -0.29, 1e-10, 0.7)])
print(OSError.__str__(OSError(2, 'x')), OSError.__str__(OSError(2, 'x', 'f')))
e = ValueError()
e.__cause__ = KeyError('k')
e.__context__ = None
print(repr(e.__cause__), e.__suppress_context__, e.__context__)
def g():
    yield 1
it = g()
next(it)
try:
    it.throw(ValueError("5"))
except ValueError as x:
    print(repr(x))
class A:
    pass
a = A()
for i in range(40):
    setattr(a, f'x{i}', i)
print(dir(a)[-3:])
print(len(re.match('(?:a|b)*c', 'ab' * 20000 + 'c').group()))
try:
    print(**5)
except TypeError as x:
    print(x)
print(repr([len, abs]) == repr([len, abs]), '%a' % [1.5, 'é'])
import sys
for o, n in (({}, 'kees'), (1.5, 'hexx'), ((), 'cont'), (1j, 'imagg')):
    try:
        getattr(o, n)
    except AttributeError as e:
        print(e, getattr(e, 'name', None))
{}.kees

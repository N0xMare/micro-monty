# math: values, special cases, errors and the integer functions.
import math


def attempt(f):
    try:
        print(repr(f()))
    except Exception as e:
        print(type(e).__name__, e)


print(math.pi, math.e, math.tau, math.inf, -math.inf, math.nan)
xs = [0.0, -0.0, 0.5, 1.0, 2.0, 10.0, 0.1, 1e-10, 123.456, 1e300]
for name in ['sqrt', 'exp', 'log', 'log2', 'log10', 'log1p', 'expm1', 'sin', 'cos', 'tan',
             'atan', 'sinh', 'cosh', 'tanh', 'asinh', 'floor', 'ceil', 'trunc', 'fabs']:
    f = getattr(math, name)
    out = []
    for x in xs:
        try:
            out.append(repr(f(x)))
        except (ValueError, OverflowError) as e:
            out.append(type(e).__name__)
    print(name, ' '.join(out))
for name in ['asin', 'acos', 'atanh']:
    f = getattr(math, name)
    print(name, [f(x) for x in [-0.5, 0.0, 0.25, 0.9]])
print(math.acosh(1.0), math.acosh(10.0), math.atan2(1, -1), math.atan2(-0.0, -1.0))
print(math.pow(2, 0.5), math.pow(-8, 3), math.pow(0.0, -0.0), math.pow(1.0, math.nan), 2 ** 0.5)
print(math.hypot(3, 4), math.hypot(1e200, 1e200), math.dist((0, 0), (3, 4)), math.hypot())
print(math.fsum([0.1] * 10), sum([0.1] * 10), math.fsum([1e100, 1.0, -1e100]), math.prod([1, 2, 3, 4]))
print(math.copysign(3, -0.0), math.fmod(-7, 3), math.remainder(7, 3), math.modf(-2.5), math.frexp(1e-310))
print(math.ldexp(0.75, 3), math.ldexp(1.0, -1074), math.ldexp(1.0, -1075), math.ldexp(1.0, 2**70 * 0))
print(math.isfinite(1e308), math.isinf(-math.inf), math.isnan(math.nan), math.isclose(1.0, 1.0 + 1e-10))
print(math.degrees(math.pi), math.radians(180), math.gamma(5.5), math.lgamma(100.0), math.gamma(-1.5))
print(math.erf(0.5), math.erfc(0.5), math.erf(-3.0), math.erfc(3.0), math.cbrt(64.0), math.cbrt(-1000.0))
print(math.exp2(10), math.nextafter(1.0, 2.0), math.ulp(1.0), math.ulp(0.0), math.fma(2.0, 3.0, 1.0))

# integer functions and big arguments
print(math.factorial(20), math.factorial(30), math.comb(50, 25), math.perm(10, 3), math.perm(5))
print(math.comb(2**70, 2), math.perm(2**70, 2), math.gcd(2**100, 6**50), math.lcm(4, 6, 2**70))
print(math.isqrt(10**40 + 1), math.isqrt(0), math.gcd(), math.gcd(0, -12), math.lcm())
print(math.floor(2**80 + 0.0) == 2**80, math.ceil(-1.5), math.trunc(-2.7), math.floor(10**30))
print(math.log(2**1000), math.log2(2**2000), math.log10(10**400), math.log(10**30, 10))
print(math.sumprod([1, 2, 3], [4, 5, 6]), math.sumprod([0.1, 0.2], [3, 4]))

# errors
attempt(lambda: math.sqrt(-1))
attempt(lambda: math.log(0))
attempt(lambda: math.log(-(2**100)))
attempt(lambda: math.exp(1000))
attempt(lambda: math.factorial(-1))
attempt(lambda: math.factorial(2.5))
attempt(lambda: math.factorial(2**64))
attempt(lambda: math.comb(-1, 2))
attempt(lambda: math.isqrt(-1))
attempt(lambda: math.acos(2))
attempt(lambda: math.sqrt('x'))
attempt(lambda: math.floor(math.inf))
attempt(lambda: math.fsum([math.inf, -math.inf]))
attempt(lambda: math.gamma(0))
attempt(lambda: math.fma(math.inf, 0, 1))
attempt(lambda: math.sumprod([1], [1, 2]))

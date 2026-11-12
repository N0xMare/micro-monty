# complex numbers and cmath: formatting, signed zeros, mixed-mode
# arithmetic, special values and errors.
import cmath

inf = float('inf')
nan = float('nan')

for z in [1j, -1j, 0j, -0j, complex(-0.0, -0.0), complex(0.0, -0.0), 1 + 2j,
          complex(1e16, -1e-5), complex(inf, nan), complex(-inf, 0), 3.5 - 0j,
          complex(1, 1e300) * 1e10, complex('  ( 1.5-2.25J ) ')]:
    print(repr(z), str(z), z.real, z.imag, z.conjugate(), abs(z) if z == z else 'nan')

z = complex(3.14159, -2.5)
for spec in ['', '.2f', '10.3', '<30', '^+30.4e', '*>25g', '.0f', 'e', '_.3f', ',.2f', '-.1%' if False else '.1g']:
    print(spec, '|' + format(z, spec) + '|')
print(f'{z:.3}', f'{1j:g}', f'{complex(0, -0.0):}', f'{-1e-7j:.2e}')

# Mixed-mode: a real operand only touches its own part.
print(complex(inf, 0) * 2, 2 * complex(inf, 0), complex(0, inf) + 1.0)
print(1j ** 2, (1 + 1j) ** 3, (2j) ** -1, 0j ** 0, (1 + 2j) ** 0.5)
print((-8) ** (1 / 3), (-1) ** 0.5, (-2.0) ** 1.5)
print(1j == 1j, 1 + 0j == 1, 1 + 0j == True, 2j != 2, hash(1 + 0j) == hash(1), hash(3j) == hash(complex(0, 3)))
print(bool(0j), bool(-0j), bool(1e-300j), -(1 + 1j), +(1 - 1j))
print(complex(1, 2) / complex(3, -4), divmod.__name__, 7 / (1 + 1j))

for bad in [lambda: 1j < 2j, lambda: 1j // 2, lambda: 1j % 2, lambda: 1 / 0j,
            lambda: 0j ** -1, lambda: pow(1j, 2, 3), lambda: int(1j), lambda: float(2j),
            lambda: complex('1+'), lambda: complex('j1'), lambda: complex(1, '2'),
            lambda: complex('1', 2), lambda: round(1j), lambda: complex(10 ** 400)]:
    try:
        print(bad())
    except Exception as e:
        print(type(e).__name__, e)


class Num:
    def __complex__(self):
        return 2 + 3j


print(complex(Num()), complex(real=1.5), complex(imag=2), complex(2, 1.5))

# cmath: values well away from subnormals (results there may differ in
# the last place).
for name in ['sqrt', 'exp', 'log', 'log10', 'sin', 'cos', 'tan', 'asin', 'acos', 'atan',
             'sinh', 'cosh', 'tanh', 'asinh', 'acosh', 'atanh']:
    f = getattr(cmath, name)
    row = []
    for w in [0.5 + 0.25j, -2 + 3j, 1j, -1, complex(-0.0, -0.0), complex(inf, 1), complex(nan, 0),
              complex(-inf, inf), complex(1, -inf)]:
        try:
            row.append(repr(f(w)))
        except ValueError as e:
            row.append('ValueError')
    print(name, ' '.join(row))
print(cmath.phase(-1), cmath.phase(complex(-1, -0.0)), cmath.polar(3 + 4j), cmath.rect(2, cmath.pi / 2))
print(cmath.rect(inf, 1), cmath.rect(0, inf), cmath.log(8, 2), cmath.log(-1, 1j), cmath.log(0, 2))
print(cmath.isfinite(1 + 1j), cmath.isinf(complex(1, inf)), cmath.isnan(complex(nan, 0)))
print(cmath.isclose(1 + 1j, 1 + 1.0000000001j), cmath.isclose(1j, 1.1j, rel_tol=0.2),
      cmath.isclose(complex(inf, 0), complex(inf, 0)), cmath.isclose(0j, 1e-10j, abs_tol=1e-9))
print(cmath.pi, cmath.tau, cmath.inf, cmath.infj, cmath.nanj, cmath.e)
for bad in [lambda: cmath.log(0), lambda: cmath.exp(1000), lambda: cmath.sqrt('x'),
            lambda: cmath.atanh(1), lambda: cmath.rect(1, inf), lambda: cmath.cosh(complex(1, inf)),
            lambda: cmath.isclose(1, 2, abs_tol=-1), lambda: cmath.log(1, 0)]:
    try:
        print(bad())
    except Exception as e:
        print(type(e).__name__, e)

# Ternary pow dispatches each numeric slot in operand order.
for args in [(1j, 2.0, 3), (1.0, 2j, 3), (1, 2, 3j), ('x', 2j, 3), (1, 2, 'x')]:
    try:
        print(pow(*args))
    except (TypeError, ValueError) as e:
        print(type(e).__name__, e)

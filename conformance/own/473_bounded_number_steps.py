# Bounded native steps for numbers (docs/spec.md §5): `int()`, `float()`
# and `complex()` read a long text a piece at a time in `_steps`, and
# convert long digits a piece at a time; a long text that is no number
# raises CPython's message (a 200-char repr for `int`, the whole repr in
# `_num_error` for `float` and `complex`). The `math` folds (`fsum`,
# `prod`, `sumprod`, `dist`, `hypot`) go on over long sequences a slice
# per step, running the whole call again in Python at a user number.
import math
import sys
from fractions import Fraction

N = 3_000_000


def show(f, *a):
    try:
        r = f(*a)
    except ValueError as e:
        m = str(e)
        print('ValueError', len(m), m[:70], m[-12:])
        return
    if isinstance(r, int):
        print('int', r.bit_length(), r % 1_000_003, str(r)[:12] if r.bit_length() < 4000 else '')
    else:
        print(repr(r))


ones = '1' * N
show(int, ones + 'x')
show(int, 'x' + ones)
show(int, ones + "'" + ones)
show(int, ones + '"' + "'")
show(int, '\u0661' * 1000 + ones + ' ')
show(int, ones.encode() + b'\xff')
show(int, ' ' * N + '-7_0 ')
show(int, ' ' * N + '-7__0')
show(int, 'f' * N, 16)
show(int, '0x_' + '1_0' * (N // 3), 0)
show(int, '01' * (N // 2), 2)
show(int, '7' * N, 8)
show(int, '0' * N + '42', 0)
show(int, '0' * N + '1', 0)
show(int, '0' * 4000 + '1', 0)
show(int, '0_' * 2000 + '0', 0)
show(int, ones)
sys.set_int_max_str_digits(0)
x = int('7' * 40_000)
print(x % 1_000_003, x.bit_length())
show(int, '9' * 20_000 + '_1', 36)
sys.set_int_max_str_digits(4300)

show(float, '0.' + '0' * N + '15e' + str(N + 1))
show(float, ' ' * N + '1_5.2_5e-1 ')
show(float, ones)
show(float, ones + '.5e-' + str(N - 300))
show(float, ones + 'x')
show(float, '1' * 70_000 + '__1')
show(float, ('1' * 70_000).encode() + b'\xa0')
show(complex, '(' + ones + 'e-' + str(N) + '-' + ones + '.5e-3000000j)')
show(complex, ones + 'j' + ' ' * N)
show(complex, ones + 'x')
show(complex, ones + '_')
show(complex, '\u3000' * 1000 + '1_5j')

xs = [((i * 7919) % 10007) / 7.0 - 700 for i in range(400_000)]
print(math.fsum(xs), math.fsum(iter(xs)), math.fsum([1e308, -1e308] * 3))
try:
    math.fsum([1e308] * 400_000)
except OverflowError as e:
    print('OverflowError', e)
print(math.prod([1.0000001] * 300_000), math.prod([1, -1] * 150_000, start=3))
print(math.prod([1] * 300_000 + [Fraction(1, 3)], start=2))
ys = [float(i % 13) for i in range(300_000)]
ns = [i % 13 for i in range(300_000)]
print(math.sumprod(xs[:300_000], ys), math.sumprod(ns, ns), math.sumprod(ns, ys))
print(math.sumprod(ns + [Fraction(1, 2)], ns + [3]))
print(math.dist(xs[:200_000], ys[:200_000]), math.hypot(*xs[:200_000]))
print(math.dist([5e-324] * 300_000, [0.0] * 300_000), math.hypot(*([1e-310] * 300_000)))
try:
    math.dist(xs, ys)
except ValueError as e:
    print('ValueError', e)

# `float.fromhex` of a long text reads it in one go, a word at a time; an
# overflow is reported before junk after the number, as CPython does.
for t in ['0x1.' + '0' * N + '1p0', ' ' * N + '-0x1.8p1 ', '0x' + '0' * N + '.' + 'f' * N + 'p-4',
          '0x1' + '0' * N + 'p-' + str(4 * N), '1' * N + 'p0', '0x1p99999x', '0x1.' + '0' * N + 'p0z',
          ' ' * N + 'inf', ' ' * N + '-Infinity ', ' ' * N + 'nanx', '0x8' + '0' * N + '1p-' + str(4 * N + 1077)]:
    try:
        print(float.fromhex(t).hex())
    except (ValueError, OverflowError) as e:
        print(type(e).__name__, e)
for x, n, order in [(-5, N, 'big'), (-(2 ** 100), N + 1, 'little'), (7, N, 'big'), (-1, 70, 'little')]:
    b = x.to_bytes(n, order, signed=True)
    print(len(b), b[:14].hex(), b[-14:].hex(), b.count(255))
print(hex(3 ** 5000)[-20:], oct(-(3 ** 5000))[:20], bin(3 ** 5000).count('1'))

# Non-ASCII words through the word-at-a-time scans (space, hex digits,
# exponent digits, trailing space): an invalid text, never a wrapped sum.
for t in ['0x' + '\u00e9' * 8, '0x1p' + '\u00e9' * 4, '0p' + chr(0xfffd) * 2, '\u00e9' * 8 + '0x1p0',
          '0x1p0' + '\u00e9' * 8, '0x1p1' + '\u00e9' * 8, '0x1.' + '\u00e9' * 8, '\u3000' * 8 + '0x1p0',
          '0x1p0' + ' ' * 7 + '\u00e9' * 8]:
    try:
        print(float.fromhex(t).hex())
    except ValueError as e:
        print(type(e).__name__, e)

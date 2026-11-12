# round(float, ndigits) with ndigits < 0 is correctly rounded, as CPython's
# dtoa does it: the float's exact value to a multiple of 10**-ndigits, half
# to even, then the nearest float.
import math
import zlib

print(round(1e40, -1), round(1.9676527011956854e+23, -1))
print(round(25.0, -1), round(35.0, -1), round(-25.0, -1), round(-5.0, -1))
print(round(5e22, -22), round(15e22, -22), round(2.5e-5, -1), round(-0.0, -2))
print(round(123456789.0, -3), round(1.4e308, -308), round(5e-324, -1))
try:
    round(1.7976931348623157e308, -308)
except OverflowError as e:
    print("OverflowError:", e)

# Pseudo-random floats over the whole exponent range, against CPython.
x = 12345
out = []
for i in range(3000):
    x = (x * 6364136223846793005 + 1442695040888963407) % 2**64
    m = x >> 11
    e = (x % 2097) - 1074
    f = math.ldexp(m, e - 52)
    nd = -1 - (x >> 3) % 320
    if i % 2:
        f = -f
    try:
        out.append(repr(round(f, nd)))
    except OverflowError:
        out.append("overflow")
    # Near the magnitude of f, where the rounding is not trivially 0.
    k = int(math.log10(abs(f))) if f else 0
    for d in (k, k + 1, k - 1, k - 15, k - 17):
        if d > 0:
            try:
                out.append(repr(round(f, -d)))
            except OverflowError:
                out.append("overflow")
print(len(out), zlib.crc32(" ".join(out).encode()))

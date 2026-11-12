# Corners of the codemode stdlib round: statistics on Decimals, Fraction as
# numbers.Rational, global_enum flag reprs, adler32 start values,
# HTMLCalendar pages, BytesIO.seek arguments and hmac on bytearrays.
import calendar
import enum
import hmac
import io
import numbers
import statistics
import zlib
from decimal import Decimal as D
from fractions import Fraction as F

data = [D('1.1'), D('2.2'), D('3.3')]
print(statistics.mean(data), repr(statistics.stdev(data)), repr(statistics.variance(data)))
print(repr(statistics.pvariance(data)), repr(statistics.harmonic_mean([D('1'), D('4')])))
print(statistics.median(data), statistics.mode([D('1.5'), D('1.5'), D('2')]))
print(isinstance(F(1, 2), numbers.Rational), isinstance(F(1, 2), numbers.Real))
print(D('0.5') == F(1, 2), F(1, 2) == D('0.5'), D('0.5') < F(2, 3), sorted([D('0.7'), F(1, 2)]))


@enum.global_enum
class Perm(enum.Flag):
    R = 4
    W = 2
    X = 1


print(repr(Perm.R), repr(Perm.R | Perm.W), str(Perm.R | Perm.X))

print([zlib.adler32(b'', v) for v in (0, 1, 65520, 65521, 65535, 0xFFFF0000, 0xFFFFFFFF)])
print([zlib.adler32(b'abc', v) for v in (0, 65521, 0xFFFFFFFF)])

page = calendar.HTMLCalendar().formatyearpage(2024)
print(type(page).__name__, page[:38])

b = io.BytesIO(b'0123456789')
print(b.seek(True), b.seek(3, 1), b.seek(-2, 2), b.read())
for args in ((1.5,), (0, 1.0), ('a',), (2 ** 70,)):
    try:
        b.seek(*args)
    except (TypeError, OverflowError) as e:
        print(type(e).__name__, e)

key, msg = bytearray(b'key'), bytearray(b'message')
h = hmac.new(key, msg, 'sha256')
h.update(bytearray(b'!'))
print(h.hexdigest() == hmac.new(b'key', b'message!', 'sha256').hexdigest())
print(hmac.digest(key, msg, 'sha256') == hmac.digest(b'key', b'message', 'sha256'))

# The globals() view under `|` from the left and reversed().
last_name = 1
print(next(reversed(globals())), 'last_name' in ({} | globals()), type({} | globals()).__name__)

# fractions, statistics and uuid (the deterministic parts).
import statistics
import uuid
from fractions import Fraction

a, b = Fraction(3, 4), Fraction('-1.25')
print(a + b, a - b, a * b, a / b, a // b, a % b, a ** 2, a ** -1, -a, abs(b), divmod(a, Fraction(1, 3)))
print(repr(Fraction(10, -4)), Fraction(0.1).limit_denominator(100), Fraction('3e-2'), Fraction(7, 3).__round__(), round(Fraction(7, 3), 2))
print(a < 0.8, a == 0.75, a + 1, 1 - a, a * 0.5, float(a), int(b), Fraction(5).is_integer(), a.as_integer_ratio())
print(format(Fraction(1, 3), '.3f'), format(Fraction(22, 7), '>12'), f'{Fraction(1, 8):.2%}', format(Fraction(-1, 3), 'e'))
for bad in [lambda: Fraction(1, 0), lambda: Fraction('x'), lambda: Fraction(1.5, 2)]:
    try:
        bad()
    except (ZeroDivisionError, ValueError, TypeError) as e:
        print(type(e).__name__, e)
d = [2.5, 3.25, 5.5, 11.25, 3.25, 2]
print(statistics.mean(d), statistics.median(d), statistics.mode(d), statistics.stdev(d), statistics.pvariance(d))
print(statistics.median_grouped([1, 2, 2, 3, 4]), statistics.quantiles(range(1, 11), n=5), statistics.quantiles(d, method='inclusive'))
print(statistics.mean([Fraction(1, 3), Fraction(2, 3), 1]), statistics.covariance([1, 2, 3], [1, 3, 2]), statistics.correlation([1, 2, 3], [3, 2, 1], method='ranked'))
n = statistics.NormalDist.from_samples([1, 2, 3, 4])
print(n.mean, n.stdev, n.overlap(statistics.NormalDist(3, 1)), n.quantiles(4), n.zscore(4))
print(statistics.multimode([1, 1, 2, 2, 3]), statistics.harmonic_mean([2, 4], weights=[1, 3]), statistics.fmean([1, 2], weights=[3, 1]))
try:
    statistics.stdev([1])
except statistics.StatisticsError as e:
    print(e)
u = uuid.uuid5(uuid.NAMESPACE_URL, 'https://example.com')
print(u, u.version, u.variant, u.hex, u.int, u.fields, u.bytes_le, uuid.UUID(bytes=u.bytes) == u)
print(uuid.uuid3(uuid.NAMESPACE_OID, '1.2.3'), uuid.UUID('{12345678-1234-5678-1234-567812345678}').time_low, uuid.uuid8(1, 2, 3))
print(uuid.uuid4().version, len(uuid.uuid4().hex), uuid.uuid7().version, uuid.uuid1().version, uuid.UUID(int=1) < uuid.UUID(int=2))

# Big integers: promotion, signs, division, shifts, bitwise, pow, hash,
# conversions and the 4300-digit limit.


def attempt(f):
    try:
        print(repr(f()))
    except Exception as e:
        print(type(e).__name__, e)


big = 2**100 + 12345
neg = -(3**70)
print(big, neg, -big, abs(neg), 2**63, -(2**63), 2**64 - 1)
print(2**47 - 1 + 1, 2**47 * 2, (2**62) * 4, -(2**47) - 1, 9223372036854775807 + 1)
print(big + neg, big - neg, big * neg, neg * neg, big * 0, 0 * neg)

# floor division and modulo signs
for a in [big, -big, 7, -7, 2**64, -(2**64)]:
    for b in [3, -3, 2**70 + 1, -(2**70 + 1), 2**32]:
        print(a // b, a % b, divmod(a, b), (a // b) * b + a % b == a)
attempt(lambda: big // 0)
attempt(lambda: big % 0)
attempt(lambda: divmod(big, 0))
print(big / 3, -big / 7, (10**400) / (10**399), 1 / big)
attempt(lambda: (10**400) / 3)
attempt(lambda: 1 / 0)

# shifts by large amounts
print(1 << 100, big >> 50, -big >> 50, big >> 1000, -big >> 1000, -1 >> 200)
print(5 << 0, big << 3, -big << 7, (2**64) >> 64, (-(2**64)) >> 63)
attempt(lambda: 1 << -1)
attempt(lambda: big >> -1)
attempt(lambda: 1 << (2**100))
print(0 << (2**100), 1 >> (2**100), -1 >> (2**100))

# bitwise with negatives
for a in [big, -big, 2**70 - 1, -(2**70)]:
    for b in [0, -1, 2**65 + 3, -(2**65 + 3), 255]:
        print(a & b, a | b, a ^ b)
print(~big, ~-big, ~(2**64), ~0)

# pow
print(2**200, (-2) ** 101, 10**50, 7**0, 0**0, (-1) ** (2**70), 2**-2, (2**70) ** 2)
print(pow(3, 2**70, 10**9 + 7), pow(-7, 123, 2**70), pow(2**100, 3, -97), pow(3, -1, 2**61 - 1))
attempt(lambda: pow(2, -1, 4))
attempt(lambda: pow(2, 3, 0))
attempt(lambda: pow(2**100, 0.5))

# comparisons, mixed with float
print(big > 2**100, big == big + 0, -big < neg, 2**53 + 1 == 2.0**53, 2**53 + 1 > 2.0**53)
print(10**400 > 1e308, -(10**400) < -1e308, 10**400 == float('inf'), 2**64 == 2.0**64)
print(max(big, 3, neg), min([big, neg, 0]), sorted([big, -big, 1, 2**64]))

# hash consistent with CPython's numeric hash
for x in [2**61 - 1, 2**61, 2**61 + 1, -(2**61), 2**64, -(2**100), 2**200 + 5, 10**30]:
    print(hash(x))
print(hash(2.0**70) == hash(2**70), {2**64: 'a'}[2.0**64], {big: 1} == {big + 0: 1})

# int <-> str, bases and the digit limit
print(int('123456789012345678901234567890'), int('-0x' + 'f' * 30, 16), int('1_000_000_000_000_000_000_000'))
print(int('z' * 20, 36), int('  -101  ', 2), hex(big), oct(neg), bin(2**70), hex(-(2**64)))
print(int(str(10**4299)) == 10**4299, len(str(10**4299)), len(str(-(10**4299))))
attempt(lambda: str(10**4300))
attempt(lambda: int('1' * 4301))
attempt(lambda: repr(-(10**5000)))
attempt(lambda: f'{10**4300}')
print(int('1' * 5000, 16) > 0, len(hex(10**5000)))
attempt(lambda: int('12a'))
attempt(lambda: int('', 16))

# int <-> float with rounding and overflow
print(float(2**53), float(2**53 + 1), float(2**53 + 2), float(2**53 + 3), float(-(2**53 + 1)))
print(float(2**1023), float(2**1024 - 2**971), float(2**1024 - 2**970 - 1), float(2**63 - 1), float(2**64 + 2**11 + 1))
attempt(lambda: float(2**1024))
attempt(lambda: float(2**1024 - 2**970))
attempt(lambda: 2**1024 + 0.5)
print(int(1e300) == 10**300, int(1e300), int(-2.5e20), int(2.0**80), int(1e15 + 0.5))
attempt(lambda: int(float('inf')))
attempt(lambda: int(float('nan')))
print(big * 1.5, big + 0.5, 2**70 / 2**10, (2**80) // 3.5, (2**80) % 3.0)

# methods
print(big.bit_length(), (-big).bit_length(), (2**64).bit_count(), (0).bit_length())
print((2**70).to_bytes(9, 'big'), (-(2**70)).to_bytes(10, 'little', signed=True))
print(int.from_bytes(b'\x01' + b'\x00' * 10, 'big'), int.from_bytes(b'\xff' * 10, 'little', signed=True))
attempt(lambda: (2**70).to_bytes(8, 'big'))
attempt(lambda: (-1).to_bytes(2, 'big'))
print(big.as_integer_ratio(), (2**70).conjugate(), round(big, -5), round(2**70, -22))
print(abs(-(2**64)), -(-(2**64)), +big, bool(2**64), bool(big - big))
print(sum([2**62, 2**62, 2**62]), sum(range(2**40, 2**40 + 10)), 2**62 * 2**62)
attempt(lambda: [0][2**64])
attempt(lambda: 'abc' * (2**64))
# Fuzzer regression: a negative count past the index range overflows too.
for seq in ('abc', b'ab', [1], (1,)):
    attempt(lambda: seq * -(2**64))
    attempt(lambda: -(2**100) * seq)
    print(seq * -(2**63))
# Fuzzer regression: int / int past 2**53 rounds once, not per operand.
print(2305843009213691764 / 10, 847288609443 / 50031545098999707)
print(123456789012345678 / 1000, -(2**62 + 1) / 3, 3 / (2**60 + 1), (2**53 + 1) / 1)
print('abc'[: 2**70], [1, 2, 3][-(2**70):], range(2**40)[5], len(range(10**3)))

# Fuzzer regressions: rounding must not saturate at i64 or lose signed zero.
for x in (1.5, -1.5, 2.5, -2.5, 2.0**63, -(2.0**63), 1e300):
    print(round(x))
print(round(-0.1, -1), (-0.0) ** 0.5)
attempt(lambda: chr(2**100))
attempt(lambda: int('x' * 300))
try:
    1e300 ** 2
except OverflowError as e:
    print(e.args)

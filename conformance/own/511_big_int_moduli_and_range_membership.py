# An exact int past i64 is in no range whose bounds fit i64, at once (it
# used to list the range: 1.43 s and 66 MB, or MemoryError); anything
# else is compared with each item, as CPython's iterative search does.
import math


class E(int):
    def __eq__(self, o):
        return True

    __hash__ = int.__hash__


print((1 << 70) in range(10**7), -(1 << 70) in range(-10**7, 10), (1 << 70) not in range(10**7))
print(1.5 in range(100_000), 99_999.0 in range(100_000), E(-1) in range(3), True in range(3), 3 in range(1, 10**18, 2))
print(range(10**6).count(1 << 70), range(10**6).index(999_999))
# Division by a modulus of one or two 32-bit limbs is a short division
# by the 64-bit word; the results are CPython's at every limb boundary.
mods = [1, 2, 3, 7, (1 << 31) - 1, 1 << 31, (1 << 32) - 1, 1 << 32, (1 << 32) + 1, (1 << 33) + 7,
        (1 << 62) - 1, 1 << 63, (1 << 63) + 1, (1 << 64) - 1, 1 << 64, (1 << 64) + 1, (1 << 96) - 1, True]
seed = 12345


def rnd(bits):
    global seed
    out = 0
    for _ in range(bits // 61 + 1):
        seed = (seed * 6364136223846793005 + 1442695040888963407) % (1 << 64)
        out = (out << 61) | (seed >> 3)
    return out >> (out.bit_length() - bits) if out.bit_length() > bits else out


for bits in (1, 31, 32, 33, 63, 64, 65, 95, 96, 97, 127, 128, 129, 1000, 4096, 65_537):
    a = rnd(bits) | (1 << (bits - 1))
    row = []
    for m in mods:
        q, r = divmod(a, m)
        nq, nr = divmod(-a, m)
        row.append((q * m + r == a and 0 <= r < m, hash((q, r, nq, nr, a % -m, a // -m)) % 1_000_003))
        x = a
        x %= m
        row.append(x == r and pow(a, 1, m) == r and pow(a, 2, m) == r * r % m)
        row.append(math.gcd(a, m) % 1_000_003)
    print(bits, row)
# The largest quotient words: a dividend of all ones, and the modulus less one.
for m in mods[1:]:
    a = (1 << 4096) - 1
    print(m.bit_length(), divmod(a, m)[1], divmod(m - 1, m), divmod(m * m - 1, m), math.gcd(a, m))
# Big operands at a quarter of the int cap, a short division each.
a = (1 << (1 << 24)) - 12345
for m in (3, (1 << 62) - 1, (1 << 64) - 1, 1 << 64):
    print(m.bit_length(), a % m, (-a) % m, pow(a, 2, m), math.gcd(a, m), (a // m).bit_length())

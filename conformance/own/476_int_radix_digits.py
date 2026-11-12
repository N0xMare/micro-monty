# bin/oct/hex write a heap int's digits straight into the result: every
# byte boundary, the top byte's leading zeros, signs, and the ints that
# are not plain ints.
from enum import IntEnum, IntFlag


class Color(IntEnum):
    RED = 5


class Perm(IntFlag):
    R = 4
    W = 2


class Idx:
    def __index__(self):
        return -77


vals = [0, 1, -1, True, False, 7, 8, 255, 256, -256, 2**63 - 1, -2**63, 2**63, 2**64 - 1, 2**64]
for b in (63, 64, 65, 127, 128, 129, 255, 256, 257, 4095, 4096, 4097, 32767, 32768, 32769):
    vals += [1 << b, (1 << b) - 1, -(1 << b), -((1 << b) - 1), (1 << b) // 3, -(5 << b) // 7]
for x in vals:
    for f in (bin, oct, hex):
        s = f(x)
        print(f.__name__, len(s), s[:40], s[-40:], int(s, 0) == x)
for x in (Color.RED, Perm.R | Perm.W, Idx()):
    print(bin(x), oct(x), hex(x))
for bad in ('a', 1.5, None, [1]):
    for f in (bin, oct, hex):
        try:
            f(bad)
        except TypeError as e:
            print(f.__name__, e)
# One past a chunk of the reader: 4096 bytes of digits, then more.
x = (1 << 40000) // 7
print(len(bin(x)), len(oct(x)), len(hex(x)), int(bin(x), 2) == x, int(oct(x), 8) == x)

# `math.gcd` and `math.lcm` over many large arguments go on pairwise in
# the prelude, a native step per pair: 40 ints at the int cap once ran in
# one 659 ms step. `lcm` divides the shorter operand by the gcd, not the
# product, and two ints past a quarter of a step's work go to the prelude
# as a gcd, a division and a product, a step each. Results, signs, zeros
# and the conversion order are CPython's.
import math
import random

rng = random.Random(515)


class I:
    def __init__(self, v):
        self.v = v

    def __index__(self):
        print('index', self.v)
        return self.v


def digest(v):
    return (v.bit_length(), v % 1_000_003)


for trial in range(300):
    n = rng.randrange(0, 7)
    xs = []
    for _ in range(n):
        bits = rng.choice((0, 3, 31, 32, 33, 64, 65, 127, 129, 300, 2000))
        x = rng.getrandbits(bits) if bits else 0
        if rng.random() < 0.3:
            x = -x
        if xs and rng.random() < 0.3:
            x *= rng.choice(xs) or 1
        xs.append(x)
    print(trial, digest(math.gcd(*xs)), digest(math.lcm(*xs)))

big = (1 << 20_000_000) - 1
# No two cap-sized values meet in one pair: their estimate refuses them.
for args in ([3] + [big] * 12, [big, -6] * 4, [6, big, 0, big, 10, -big], [0, 0, 15, big, big]):
    print(len(args), digest(math.gcd(*args)))
for args in ([big] + [3] * 12, [big, 0] + [6] * 5, [-big, 10, 15, 21, 2], [big, 7, 49, -14]):
    print(len(args), digest(math.lcm(*args)), digest(math.lcm(*reversed(args))))
print(math.gcd(I(12), big, I(18)), math.lcm(I(4), 6, I(10)))
for bad in ((big, 6, 'x', I(3)), (big, I(3), 2.5)):
    for f in (math.gcd, math.lcm):
        try:
            f(*bad)
        except TypeError as e:
            print('TypeError', e)

cap = (1 << 67_108_800) - 12_345
for a, b in ((cap, 3), (-cap, 3), (3, -cap), (cap, 0), (0, -cap), (cap, True), (False, cap), (cap, 7 * 2**40), (-6, cap * 5)):
    r = math.lcm(a, b)
    print(type(r).__name__, digest(r), r == math.lcm(b, a))

"""An input of bench/hot_set.py, not a gated bench program: integer
arithmetic past 64 bits as the recorded sessions' hash reimplementations
do it (conformance/codemode/real, the dict-review prompt): products and
rotations masked to 64 bits, values past the i64 range, powers of two and
compares against them. No bench program reaches these paths, and with
their helper calls out of line those sessions rose up to 1.3% [M]."""

P1 = 11400714785074694791
P2 = 14029467366897019727
P5 = 2870177450012600261
MASK = (1 << 64) - 1
MOD = (1 << 61) - 1


def xx(lanes):
    acc = P5
    for lane in lanes:
        acc = (acc + (lane & MASK) * P2) & MASK
        acc = ((acc << 31) | (acc >> 33)) & MASK
        acc = (acc * P1) & MASK
    acc = (acc + (len(lanes) ^ P5)) & MASK
    return acc - 2**64 if acc >= 2**63 else acc


def mod_hash(n):
    x = 0
    while n:
        x = ((x << 28) & MOD) | (x >> 33)
        x += n & 0xFFFFFFF
        n >>= 28
        if x >= MOD:
            x -= MOD
    return x


total = 0
for i in range(15000):
    total ^= xx([i, i * P1, -i, 2**63 + i])
    total ^= mod_hash(i * P2 * P2)
    total = (total + 3 ** (i % 90 + 40)) & MASK
print(total)

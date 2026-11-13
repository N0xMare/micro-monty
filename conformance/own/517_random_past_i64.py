# `random` past i64: `randint`, `randrange` and `_randbelow` with widths
# from 2**63 draw what CPython draws (the native `_mt_below` takes widths
# below 2**63; wider ones draw `getrandbits` in the prelude), as do
# `getrandbits(k)` for k from 2**63 (MemoryError) and a state index past
# a C long (OverflowError).
import random

r = random.Random(5)
for f in (lambda: r.randint(1, 2**64 - 1), lambda: r.randint(0, 2**63), lambda: r.randint(-2**63, 2**63 - 1),
          lambda: r.randrange(2**63), lambda: r.randrange(2**63 + 5), lambda: r.randrange(2**64), lambda: r.randrange(-2**64, 0),
          lambda: r.randrange(0, 2**64 - 1, 3), lambda: r.getrandbits(64), lambda: r._randbelow(2**64 - 1),
          lambda: r._randbelow(2**63 - 1), lambda: r._randbelow(2**63), lambda: r.randint(2**70, 2**70 + 2**63),
          lambda: r.randbytes(9), lambda: r.binomialvariate(2**63, 0.5) >= 0,
          lambda: r.seed(2**64 - 1) or r.random(), lambda: r.seed(-2**63) or r.random()):
    try:
        r.seed(5)
        print(f())
    except Exception as e:
        print(type(e).__name__, e)
for k in (2**63, 2**64 - 1, 2**64, 2**40):
    try:
        print(r.getrandbits(k).bit_length())
    except Exception as e:
        print(type(e).__name__, e)
for idx in (2**63, 2**64, 2**31, 625, -1, -2**63 - 1):
    try:
        r.setstate((3, tuple([1] * 624 + [idx]), None))
        print(r.random())
    except Exception as e:
        print(type(e).__name__, e)
for w in (2**63, 2**64 - 1, 2**32 + 7, 2**64, -1):
    try:
        r.setstate((3, tuple([w] * 624 + [624]), None))
        print(r.random())
    except Exception as e:
        print(type(e).__name__, e)
r.seed(1)
print(r.sample(range(2**62 + 5), 4), r.sample(range(-2**62, 2**62, 2**40), 3))
print(r.choice(range(2**62)), r.randrange(-2**63, 2**63, 2**32 + 1), r.randrange(2**63 - 1))
print([r.randint(2**63 - 2, 2**63 + 2) for _ in range(12)])
print([r.randrange(2**64 - 3, 2**64 + 3) for _ in range(6)])
print(r.getrandbits(63), r.getrandbits(64), r.getrandbits(65), r.getrandbits(0))
print(random.Random(2**64).random(), random.Random(-(2**63)).randint(1, 2**64 - 1))

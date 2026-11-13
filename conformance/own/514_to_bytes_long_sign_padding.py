# `int.to_bytes` of a negative int into many bytes: the sign padding of a
# megabyte or more is filled a step at a time (one 64 MiB fill took one
# native step of 41-58 ms). The bytes are the same as in one go.
M = 1 << 20


def show(b):
    n = len(b)
    print(n, b[:3].hex(), b[-3:].hex(), b.count(255), b.count(0), b[n // 2])


for x in (-1, -5, -256, -257, -(1 << 63), -(1 << 64) + 1, -(3 ** 200), -(1 << 1000)):
    for n in (M - 1, M, M + 7, 9 * M + 3):
        for order in ('big', 'little'):
            if n == 9 * M + 3 and order == 'little' and x != -5:
                continue
            b = x.to_bytes(n, order, signed=True)
            show(b)
            assert int.from_bytes(b, order, signed=True) == x
            del b

# Positive ints and small lengths keep their paths.
show((5).to_bytes(9 * M, 'big'))
show((-(1 << 23)).to_bytes(3, 'big', signed=True))
try:
    (-(1 << 8 * M)).to_bytes(M, 'big', signed=True)
except OverflowError as e:
    print('OverflowError', e)

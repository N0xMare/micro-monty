# A format that is one large `s` field (past a step's byte budget) packs
# and unpacks through a stepped copy, so a park or snapshot can land inside
# it: from bytes and bytearray, from shorter sources (zero-padded) and
# longer ones (cut), through unpack, unpack_from at an offset, iter_unpack
# and a Struct. A non-bytes argument fails before any copy.
import struct

N = 2_097_155
F = f'{N}s'
src = (bytes(range(251)) * (N // 251 + 1))[: N - 3] + b'xyz'
FULL = ('bytes', N, b'\x00\x01\x02\x03', b'.xyz', 8356, 8356)


def show(label, b, want):
    got = (type(b).__name__, len(b), b[:4], b[-4:], b.count(0), b.count(b'xyz'))
    print(label, got)
    assert got == want, (label, got)


show('exact', struct.pack(F, src), FULL)
show('bytearray', struct.pack(F, bytearray(src)), FULL)
show('short', struct.pack(F, src[: N // 3]), ('bytes', N, b'\x00\x01\x02\x03', b'\x00' * 4, 1400890, 2785))
show('empty', struct.pack(F, b''), ('bytes', N, b'\x00' * 4, b'\x00' * 4, N, 0))
show('long', struct.pack(F, src + b'tail'), FULL)
show('struct', struct.Struct(F).pack(src[5:]), ('bytes', N, b'\x05\x06\x07\x08', b'\x00' * 4, 8360, 8356))
for bad in (5, 'text', [1]):
    try:
        struct.pack(F, bad)
        raise AssertionError('packed')
    except struct.error as e:
        assert str(e) == "argument for 's' must be a bytes object", e

(out,) = struct.unpack(F, src)
show('unpack', out, FULL)
assert out == src
(out,) = struct.unpack(F, bytearray(src))
show('unpack bytearray', out, FULL)
buf = b'head' + src + b'end'
(out,) = struct.unpack_from(F, buf, 4)
show('unpack_from', out, FULL)
assert out == src
(out,) = struct.unpack_from(F, bytearray(buf), -N - 3)
show('unpack_from negative', out, FULL)
del buf
outs = [x for (x,) in struct.iter_unpack(F, src + src[::-1])]
show('iter_unpack 0', outs[0], FULL)
show('iter_unpack 1', outs[1], ('bytes', N, b'zyx.', b'\x03\x02\x01\x00', 8356, 0))
assert outs[0] == src and outs[1] == src[::-1]
del outs, out
try:
    struct.unpack(F, src[1:])
    raise AssertionError('unpacked')
except struct.error as e:
    assert str(e) == f'unpack requires a buffer of {N} bytes', e

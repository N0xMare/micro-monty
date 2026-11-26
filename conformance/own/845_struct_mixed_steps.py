# The wide field spans multiple native steps on both paths, and starts at
# an unaligned offset. A later conversion may change the original buffer,
# but not bytes already packed into the record.
import struct

b = bytearray(b'AB' * 1_000_001 + b'Z')


class Index:
    def __index__(self):
        b[:3] = b'xyz'
        return 0xFEDCBA98


fmt = '>B2100005sI3s'
wire = struct.pack(fmt, 0xA5, b, Index(), b)
assert len(wire) == 2_100_013
assert wire[:6] == b'\xa5ABABA'
assert wire[-7:] == b'\xfe\xdc\xba\x98xyz'
assert wire[2_000_001:2_000_005] == b'ABZ\0'

lead, field, tail, seen = struct.unpack(fmt, wire)
assert (lead, tail, seen) == (0xA5, 0xFEDCBA98, b'xyz')
assert field == b'AB' * 1_000_001 + b'Z' + b'\0' * 100_002
buffer = bytearray(b'prefix' + wire)
assert struct.unpack_from(fmt, buffer, -len(wire)) == (lead, field, tail, seen)

shorter = struct.pack('>I1800001sx', tail, b)
assert len(shorter) == 1_800_006
assert shorter[:8] == b'\xfe\xdc\xba\x98xyzB' and shorter[-4:] == b'ABA\0'
print(len(wire), lead, tail, seen, len(shorter))

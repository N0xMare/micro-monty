# Bounded scalar work, including a repeat with no per-item format tokens
# and a format window ending inside a count full of leading zeros.
import struct

count = 100_003
fmt = '>100003I'
xs = (1,) + (0xFEDCBA98,) * (count - 2) + (2,)
wire = struct.pack(fmt, *xs)
assert len(wire) == 4 * count
assert wire[:8] == b'\x00\x00\x00\x01\xfe\xdc\xba\x98'
assert wire[-4:] == b'\x00\x00\x00\x02'
assert struct.unpack(fmt, wire) == xs
assert struct.unpack_from(fmt, bytearray(b'x' + wire), -len(wire)) == xs
print(len(wire), wire[:8], wire[-4:])

fmt = b'<' + b'0' * 131_072 + b'2h'
wire = struct.pack(fmt, -32768, 32767)
assert wire == b'\x00\x80\xff\x7f'
assert struct.unpack(fmt, wire) == (-32768, 32767)
assert struct.unpack('@B0qQq', struct.pack('@B0qQq', 7, 2**64 - 1, -2**63)) == (7, 2**64 - 1, -2**63)
print(wire)

events = []
b = bytearray(b'ab')


class Index:
    def __index__(self):
        events.append('index')
        b[:] = b'XYZ'
        for _ in range(9_000):
            pass
        return 17


class Float:
    def __float__(self):
        events.append('float')
        return 2.5


class Truth:
    def __bool__(self):
        events.append('bool')
        return False


class Complex:
    def __complex__(self):
        events.append('complex')
        return 1-4j


fmt = '>2sI3sd?D'
wire = struct.pack(fmt, b, Index(), b, Float(), Truth(), Complex())
assert events == ['index', 'float', 'bool', 'complex']
values = struct.unpack(fmt, wire)
assert values == (b'ab', 17, b'XYZ', 2.5, False, 1-4j)
print(events, values)

events.clear()
try:
    struct.pack('I' + ' ' * 131_072 + 'z', Index())
except struct.error as e:
    assert str(e) == 'bad char in struct format'
else:
    assert False
assert not events
try:
    struct.pack('<IB', Index(), 256)
except struct.error:
    assert events == ['index']
else:
    assert False

for off in (-2**63, -2, 2**63 - 1, 2**63):
    try:
        struct.unpack_from('<I', bytearray(4), off)
    except (struct.error, OverflowError) as e:
        print(type(e).__name__, e)
    else:
        assert False

# This offset is both out of range and too close to the end for the field.
# CPython reports insufficient data first.
for unpack in (lambda buf, off: struct.unpack_from('>Q', buf, off),
               struct.Struct('>Q').unpack_from):
    try:
        unpack(b'x', -2)
    except struct.error as e:
        assert str(e) == 'not enough data to unpack 8 bytes at offset -2'
    else:
        assert False

import struct


class Offset:
    def __init__(self, value):
        self.value = value

    def __index__(self):
        return self.value


buf = b'azxyq'
fmt = '<4s'
for unpack in (lambda buf, offset: struct.unpack_from(fmt, buf, offset),
               struct.Struct(fmt).unpack_from):
    assert unpack(buf, Offset(-4)) == (b'zxyq',)
    for offset in (1 << 63, -(1 << 63) - 1, 1 << 64):
        try:
            unpack(buf, Offset(offset))
        except OverflowError as e:
            assert str(e) == 'Python int too large to convert to C ssize_t'
        else:
            raise AssertionError('offset outside ssize_t accepted')

    for offset, message in (
        (-(1 << 63), 'offset -9223372036854775808 out of range for 5-byte buffer'),
        ((1 << 63) - 1, 'unpack_from requires a buffer of at least 9223372036854775811 '
         'bytes for unpacking 4 bytes at offset 9223372036854775807 (actual buffer size is 5)'),
    ):
        try:
            unpack(buf, offset)
        except struct.error as e:
            assert str(e) == message
        else:
            raise AssertionError('offset outside buffer accepted')

# struct: native pack/unpack with CPython 3.14's sizes, alignment, conversions
# and messages (docs/spec.md §4, §12).
import struct
def t(f):
    try:
        r = f()
        print('ok', r)
    except Exception as e:
        print(type(e).__module__, type(e).__name__, e)
for c in 'xcbB?hHiIlLqQnNefdspPFD':
    t(lambda: struct.calcsize(c))
    t(lambda: struct.calcsize('<' + c))
for c in 'bBhHiIlLqQnN':
    t(lambda: struct.pack(c, 10**20)); t(lambda: struct.pack(c, -1 - 10**20)); t(lambda: struct.pack(c, 'x')); t(lambda: struct.pack(c, 1.0))
    t(lambda: struct.pack('<' + c, 2**70)); t(lambda: struct.pack('<' + c, -2**70))
for c in 'efd':
    t(lambda: struct.pack(c, 'x')); t(lambda: struct.pack(c, 1e300)); t(lambda: struct.pack(c, 10**400))
t(lambda: struct.pack('c', b'ab')); t(lambda: struct.pack('c', 'a')); t(lambda: struct.pack('s', 'a')); t(lambda: struct.pack('p', 'a'))
t(lambda: struct.pack('2i', 1)); t(lambda: struct.pack('i', 1, 2)); t(lambda: struct.pack('z', 1)); t(lambda: struct.pack('<P', 1))
t(lambda: struct.unpack('i', b'abc')); t(lambda: struct.unpack_from('i', b'abcdef', 5)); t(lambda: struct.unpack_from('i', b'abcdef', -10))
t(lambda: struct.unpack_from('i', b'abcdef', 10)); t(lambda: struct.iter_unpack('i', b'abc')); t(lambda: struct.iter_unpack('', b''))
t(lambda: struct.calcsize('3')); t(lambda: struct.calcsize('99999999999999999999i')); t(lambda: struct.calcsize(' i i ')); t(lambda: struct.calcsize(5))
t(lambda: struct.pack(b'<i', 1)); t(lambda: struct.Struct('i').pack(1)); t(lambda: struct.Struct(b'i').format);
t(lambda: struct.unpack('i', 'abcd')); t(lambda: struct.pack('i')); t(lambda: struct.pack()); t(lambda: struct.calcsize('i<'))
t(lambda: struct.pack('?', [])); t(lambda: struct.pack('e', 65520.0)); t(lambda: struct.pack('f', 1e39)); t(lambda: struct.pack('<e', float('nan')))
t(lambda: repr(struct.Struct('<i'))); t(lambda: struct.Struct('<2h').size); t(lambda: struct.error.__module__); t(lambda: struct.error.__mro__)
t(lambda: struct.pack('F', 'x')); t(lambda: struct.pack('D', 10**400)); t(lambda: struct.pack('F', 1e300j)); t(lambda: struct.pack('d', 1j))
t(lambda: struct.unpack_from('i', b'abcdef', -2)); t(lambda: struct.unpack_from('i', b'abcdef', offset=1)); t(lambda: struct.unpack_from('2s', b'abcdef', 1.0))
t(lambda: struct.unpack_from('2s', b'abcdef', -3))
class I:
    def __index__(self): return 7
class Fl:
    def __float__(self): return 2.5
class B:
    def __bool__(self): raise ValueError('nobool')
class Sub(int): pass
t(lambda: struct.pack('i', I())); t(lambda: struct.pack('d', I())); t(lambda: struct.pack('d', Fl())); t(lambda: struct.pack('i', Fl())); t(lambda: struct.pack('?', B()))
t(lambda: struct.pack('F', Fl())); t(lambda: struct.pack('F', I())); t(lambda: struct.pack('<hq', Sub(3), Sub(-4))); t(lambda: struct.pack('??', I(), []))
t(lambda: struct.Struct('i').unpack(b'abcd')); t(lambda: list(struct.iter_unpack('<h', b'\x01\x00\x02\x00')))
it = struct.iter_unpack('<h', b'\x01\x00\x02\x00'); print(it.__length_hint__(), next(it), it.__length_hint__(), list(it), it.__length_hint__())
t(lambda: struct.pack('i', True)); t(lambda: struct.unpack('<q', b'\xff'*8)); t(lambda: struct.unpack('<Q', b'\xff'*8))
t(lambda: struct.Struct('<i').unpack_from(b'abcd', 0)); t(lambda: struct.Struct()); t(lambda: struct.unpack('', b''))
t(lambda: struct.calcsize('é')); t(lambda: struct.Struct('i').format); t(lambda: struct.Struct(b'<i').format)
t(lambda: struct.pack('i', 2**31)); t(lambda: struct.unpack(b'i', b'abcd'))
for f in ['ce','cF','cD','cq','cP','c?','ci','cn','cf','cd','ch','ic','i0q','5s','3x','2p','c2i','<c2i','e','!ce', '@3si', '=3si']:
    print(f, struct.calcsize(f))
nan = float('nan')
for v in [0.0, -0.0, 1.0, -2.5, 65504.0, 65519.99, 1e-8, 2**-24, 2**-24*0.5, 2**-24*0.50001, 2**-14, 6.1e-5, 1/3, float('inf'), -float('inf'), nan, -nan, 1e-310]:
    for c in '<e', '>e', '<f', '>d':
        try:
            p = struct.pack(c, v)
            print(c, v, p, struct.unpack(c, p))
        except OverflowError as e:
            print(c, v, 'OverflowError', e)
print(struct.pack('<e', 65520.0) if False else '', struct.unpack('<e', b'\x01\x7c'), struct.unpack('>e', b'\x7c\x00'), struct.unpack('<f', b'\x01\x00\x80\xff'))
print(struct.pack('5p', b'abcdefg'), struct.pack('p', b'x'), struct.pack('0p', b'x'), struct.pack('0s', b'x'), struct.pack('3s', b'a'), struct.unpack('5p', b'\x09abcd'), struct.unpack('?', b'\x02'))
print(struct.pack('>F', 1+2j), struct.unpack('<D', struct.pack('<D', 1.5-2j)), struct.pack('F', 1), struct.unpack('>D', struct.pack('>D', -3+0.25j)))
rec = struct.Struct('<4sHHI2xq?3pd')
packed = rec.pack(b'MMON', 1, 65535, 2**32 - 1, -2**63, True, b'hi', 3.25)
print(rec, rec.size, packed, rec.unpack(packed))
print(struct.pack('>bhilqBHILQ', -1, -2, -3, -4, -5, 1, 2, 3, 4, 5))
print(struct.unpack('>bhilqBHILQ', struct.pack('>bhilqBHILQ', -1, -2, -3, -4, -5, 1, 2, 3, 4, 5)))
print(struct.pack('@bhilqnNP', 1, 2, 3, 4, 5, -6, 7, 8), struct.calcsize('@bhilqnNP'))
print(struct.pack('!I', 0xdeadbeef), struct.pack('=I', 0xdeadbeef), struct.pack('I', 0xdeadbeef))
print(struct.pack(' < i  2h ', 1, 2, 3), struct.unpack('3c', b'abc'), struct.pack('3c', b'a', b'b', b'c'))
data = struct.pack('<' + 'i' * 1000, *range(1000))
print(len(data), sum(x for (x,) in struct.iter_unpack('<i', data)), struct.unpack_from('<2i', data, 4 * 998))
print(struct.pack('<Q', 2**64 - 1), struct.pack('<q', -1), struct.unpack('<n', b'\x00' * 8) if False else '', struct.unpack('@N', b'\xff' * 8))

# Corners of the byte operations that run in steps (docs/spec.md §5):
# empty and boundary cases, error messages and positions, bytearray
# results, as CPython 3.14 gives them.
import base64
import binascii
import hashlib


def show(f):
    try:
        print(repr(f()))
    except Exception as e:
        print(type(e).__name__, e)


for B in (bytes, bytearray):
    print(B.__name__)
    show(lambda: B(b'a,b,,c').split(b','))
    show(lambda: B(b'a,b,,c').rsplit(b',', 1))
    show(lambda: B(b'a,b').split(b''))
    show(lambda: B(b' a  b ').split())
    show(lambda: B(b' a  b ').split(None, 1))
    show(lambda: B(b' a  b ').rsplit(None, 1))
    show(lambda: B(b'   ').split())
    show(lambda: B(b'').split(b'x'))
    show(lambda: B(b'a\nb\r\n\rc\r').splitlines())
    show(lambda: B(b'a\nb\r\n\rc\r').splitlines(True))
    show(lambda: B(b'abcabc').replace(b'', b'-'))
    show(lambda: B(b'abcabc').replace(b'', b'-', 2))
    show(lambda: B(b'abcabc').replace(b'bc', b'', 1))
    show(lambda: B(b'aaaa').replace(b'aa', b'b'))
    show(lambda: B(b'-').join([b'a', bytearray(b'b'), b'c']))
    show(lambda: B(b'-').join([b'a', 'b']))
    show(lambda: B(b'-').join(5))
    show(lambda: B(b'abc') * 0)
    show(lambda: B(b'abcdef')[::-2])
    show(lambda: B(b'abcdef')[1:5:3])
    show(lambda: [B(b'aaaa').count(b'aa'), B(b'abc').count(b''), B(b'abc').count(b'', 2), B(b'abc').count(b'', 4), B(b'abc').count(98, 0, 1)])
    show(lambda: [B(b'abcabc').find(b'c', 3), B(b'abcabc').rfind(b'a', 1, 3), B(b'abc').find(b''), B(b'abc').rfind(b''), B(b'abc').find(b'', 4), B(b'abc').find(99)])
    show(lambda: B(b'abc').index(b'd'))
    show(lambda: B(b'abc').rindex(b'a', 1))
    show(lambda: B(b'abc').find(300))
    show(lambda: B(b'abc').find('a'))
    show(lambda: [b'' in B(b'abc'), 98 in B(b'abc'), b'bc' not in B(b'abc')])
    show(lambda: 'a' in B(b'abc'))
    show(lambda: 256 in B(b'abc'))
    show(lambda: [B(b'a-b-c').partition(b'-'), B(b'a-b-c').rpartition(b'-'), B(b'abc').partition(b'x'), B(b'abc').rpartition(b'x')])
    show(lambda: B(b'abc').partition(b''))
    show(lambda: [B(b'aBc dE').upper(), B(b'aBc dE').lower(), B(b'aBc dE').swapcase(), B(b'aBc dE').capitalize(), B(b"they're 3rd").title()])
    show(lambda: [B(b'').isalpha(), B(b'').isascii(), B(b'').islower(), B(b'Ab Cd').istitle(), B(b'AB').istitle(), B(b'1a').islower(), B(b'1').islower(), B(b'\x80').isascii()])
    show(lambda: [B(b' \txy\n ').strip(), B(b'xyx').strip(b'x'), B(b'xyx').lstrip(b'xy'), B(b'').rstrip(), B(b'aaa').strip(b'a')])
    show(lambda: B(b'\x00\xffab').translate(bytes.maketrans(b'ab', b'AB'), b'\x00'))
    show(lambda: B(b'abc').translate(None, b'b'))
    show(lambda: B(b'abc').translate(b'x'))
    show(lambda: [B(b'a\tb\tc').expandtabs(), B(b'a\tb\r\n\tc').expandtabs(3), B(b'a\tb').expandtabs(0), B(b'a\tb').expandtabs(-1)])
    show(lambda: [B(b'\x01\xab').hex(), B(b'\x01\xab\xcd').hex(':'), B(b'\x01\xab\xcd').hex(b'_', -2), B(b'').hex()])
    show(lambda: B(b'ab').hex('::'))
    for s in ('', ' ', '0a 1B\t2c', '1', '1 2', '12 3', '1 ', '0a1 ', 'zz', '0g', '0a\u00e9'):
        show(lambda: B.fromhex(s))
    show(lambda: [B(b'h\xc3\xa9').decode(), B(b'').decode(), B(b'a').decode(), B(b'\xef\xbb\xbfab').decode('utf-8-sig')])
    for enc, err, raw in (
        ('utf-8', 'strict', b'ab\xffcd'),
        ('utf-8', 'strict', b'ab\xe2\x98'),
        ('utf-8', 'strict', b'\xef\xbb\xbfa\xff'),
        ('utf-8-sig', 'strict', b'\xef\xbb\xbfa\xff'),
        ('utf-8', 'replace', b'a\xffb\xe2\x98'),
        ('utf-8', 'ignore', b'a\xffb\xe2\x98'),
        ('utf-8', 'backslashreplace', b'a\xffb'),
        ('utf-8', 'nope', b'a\xff'),
        ('utf-8', 'nope', b'a'),
        ('ascii', 'strict', b'ab\x80'),
        ('latin-1', 'strict', b'\xe9\xff'),
        ('utf-16-le', 'strict', b'a\x00\x00\xdcb\x00'),
        ('utf-16', 'replace', b'\xff\xfea\x00b'),
        ('utf-32-be', 'strict', b'\x00\x11\x00\x00'),
    ):
        show(lambda: B(raw).decode(enc, err))
for s, enc, err in (
    ('aé☃', 'latin-1', 'strict'),
    ('aé☃☃b', 'ascii', 'strict'),
    ('a☃☃', 'ascii', 'strict'),
    ('a☃b', 'ascii', 'replace'),
    ('a☃b', 'ascii', 'ignore'),
    ('a☃b\U0001d11e', 'ascii', 'backslashreplace'),
    ('a☃b', 'ascii', 'xmlcharrefreplace'),
    ('a☃b', 'ascii', 'nope'),
    ('ab', 'ascii', 'nope'),
    ('', 'utf-16', 'strict'),
    ('', 'utf-32', 'strict'),
    ('', 'utf-8-sig', 'strict'),
    ('a\U0001d11e', 'utf-16-be', 'strict'),
    ('a\U0001d11e', 'utf-32', 'strict'),
):
    show(lambda: s.encode(enc, err))
show(lambda: bytearray('é☃', 'utf-8'))
show(lambda: bytearray('é', 'latin-1'))
show(lambda: bytes('', 'utf-16'))
for raw in (b'', b'QQ==', b'QQ', b'Q', b'QUJD\n', b'=QUJD', b'QU=JD', b'QUJD==', b'QUJDRA==QQ==', b'QU==QQ', b'QUJ=D'):
    show(lambda: binascii.a2b_base64(raw))
    show(lambda: binascii.a2b_base64(raw, strict_mode=True))
show(lambda: [binascii.b2a_base64(b'abcd'), binascii.b2a_base64(b'', newline=False), base64.b64encode(b'ab')])
show(lambda: [binascii.hexlify(b'\x00\xff'), binascii.unhexlify(b'00fF'), binascii.unhexlify('abcd')])
show(lambda: binascii.unhexlify(b'0g'))
show(lambda: binascii.unhexlify(b'abc'))
show(lambda: [binascii.crc32(b''), binascii.crc32(b'abc', 5), binascii.crc_hqx(b'', 7), binascii.crc_hqx(b'abc', 0xffff)])
show(lambda: [hashlib.shake_128(b'x').digest(k).hex() for k in (0, 1, 168, 169)])
show(lambda: hashlib.shake_256(b'x').hexdigest(5))
x = bytearray(b'0123456789')
x[::3] = b'abcd'
show(lambda: x)
x[1::4] = b''
show(lambda: x)
del x[::-2]
show(lambda: x)
x.__setitem__(slice(None, None, 2), b'QR')
show(lambda: x)
x.__delitem__(slice(1, None, 2))
show(lambda: x)
y = bytearray(b'ab')
show(lambda: y.__setitem__(slice(None, None, 2), b'xyz'))
y *= 3
show(lambda: y)
y *= 0
show(lambda: y)
show(lambda: [bytes(3), bytearray(2), bytes(0)])

# Lists and tuples of ints: a bad item fails where CPython's does, past the
# first step too, and a failed extend leaves the bytearray as it was.
for B in (bytes, bytearray):
    show(lambda: B([1, True, False, 255]))
    show(lambda: B((65, 66)))
    show(lambda: B([]))
    show(lambda: B([1, 256]))
    show(lambda: B([1, -1]))
    show(lambda: B([1, 2**70]))
    show(lambda: B([1, 'a']))
long = [7] * 140_000
show(lambda: bytes(long + [300]))
show(lambda: bytearray(long + [1.5]))
z = bytearray(b'xy')
show(lambda: z.extend([70, 300]))
show(lambda: z.extend(long + [None]))
show(lambda: z.__setitem__(slice(0, 1), long + ['s']))
show(lambda: z)
z.extend(long)
show(lambda: (len(z), z[-3:]))

# Sets and frozensets of ints, read in place in table order.
for B in (bytes, bytearray):
    for S in (set, frozenset):
        show(lambda: B(S([3, 1, 2, 255, 0])))
        show(lambda: B(S()))
        show(lambda: B(S([1, 300])))
        show(lambda: B(S([1, 'a'])))
        show(lambda: len(B(S(range(140_000)))))
s = set(range(256))
s.discard(5)
show(lambda: bytes(s)[:8])
z = bytearray(b'xy')
z.extend({7, 8})
show(lambda: z)
show(lambda: z.extend(set(range(140_000))))
show(lambda: z)

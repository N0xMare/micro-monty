# hashlib, base64 and binascii.
import base64
import binascii
import hashlib

data = b'The quick brown fox jumps over the lazy dog'
for name in ('md5', 'sha1', 'sha224', 'sha256', 'sha384', 'sha512', 'sha3_256', 'sha3_512', 'blake2b', 'blake2s'):
    h = hashlib.new(name, data)
    print(name, h.hexdigest(), h.digest_size, h.block_size, h.name)
h = hashlib.sha256()
h.update(b'abc')
c = h.copy()
c.update(b'def')
print(h.hexdigest(), c.hexdigest() == hashlib.sha256(b'abcdef').hexdigest(), hashlib.md5(b'').digest())
print(hashlib.blake2b(b'x', digest_size=16, key=b'k').hexdigest(), sorted(hashlib.algorithms_guaranteed)[:4])
print(base64.b64encode(data), base64.urlsafe_b64encode(b'\xfb\xff'), base64.b32encode(b'hello'), base64.b16encode(b'\x01\xab'))
print(base64.b64decode(b'aGVsbG8='), base64.b85encode(b'hello'), base64.a85encode(b'hello'))
print(base64.b32decode('NBSWY3DP'), base64.b16decode('01AB'), base64.b85decode(base64.b85encode(data)) == data)
print(binascii.hexlify(b'\x00\xff', '-'), binascii.unhexlify('00ff'), binascii.crc32(data), binascii.crc_hqx(data, 0))
print(binascii.b2a_base64(b'hi'), binascii.a2b_base64(b'aGk=\n'), data.hex(':', 4))
for bad in [lambda: base64.b64decode('a'), lambda: binascii.unhexlify('0'), lambda: hashlib.new('nope'), lambda: base64.b32decode('x')]:
    try:
        bad()
    except (ValueError, binascii.Error) as e:
        print(type(e).__name__, e)

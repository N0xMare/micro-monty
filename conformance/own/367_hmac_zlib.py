# hmac over hashlib (new/digest/copy/compare_digest and messages) and zlib's
# checksums, crc32 and adler32, with their argument errors.
import hmac, hashlib
k = b'key'; m = b'The quick brown fox jumps over the lazy dog'
for alg in ['md5', 'sha1', 'sha256', 'sha512', 'sha3_256', 'blake2b']:
    h = hmac.new(k, m, alg)
    print(alg, h.name, h.digest_size, h.block_size, h.hexdigest())
h = hmac.new(b'x' * 200, digestmod=hashlib.sha256)
h.update(b'a'); c = h.copy(); c.update(b'b')
print(h.hexdigest(), c.hexdigest())
print(hmac.digest(k, m, 'sha256').hex(), hmac.digest(k, m, hashlib.sha1).hex())
print(hmac.compare_digest('abc', 'abc'), hmac.compare_digest(b'abc', bytearray(b'abd')))
for a, b in [('é', 'é'), ('a', b'a'), (1, b'a'), (b'a', 2)]:
    try: hmac.compare_digest(a, b)
    except TypeError as e: print('TypeError', e)
for bad in [lambda: hmac.new('k', b'm', 'sha256'), lambda: hmac.new(b'k', b'm')]:
    try: bad()
    except TypeError as e: print('TypeError', e)
import base64
sig = base64.b64encode(hmac.new(b'secret', b'payload', hashlib.sha256).digest()).decode()
print(sig)
import zlib, binascii
print(zlib.crc32(b'abc'), zlib.adler32(b'abc'), zlib.adler32(b'abc', 5), zlib.crc32(b'abc', -1), zlib.adler32(b'', 2**40+3))
print(zlib.adler32(b'\xff' * 100000), zlib.crc32(bytearray(b'xyz')), zlib.crc32(b'abc') == binascii.crc32(b'abc'))
x = 1
for i in range(0, 20000, 777): x = zlib.adler32(bytes(range(256)) * 3, x)
print(x)
for f in [lambda: zlib.crc32(), lambda: zlib.crc32('s'), lambda: zlib.crc32(b'a', 1, 2), lambda: zlib.adler32(b'a', value=1), lambda: zlib.adler32(b'a', 1.5)]:
    try: f()
    except Exception as e: print(type(e).__name__, e)
print(issubclass(zlib.error, Exception), zlib.error.__module__)

# Bounded native steps over bytes (docs/spec.md §5): byte work over long
# data goes on a step at a time in `_steps`, its result built in place.
# Sized so each operation below takes more than one step (many pieces or
# matches, windows of 256 KiB, hashes 2 MiB a step), so a park lands inside
# it; results must equal one native run's.
import base64
import binascii
import hashlib
import math

data = bytes(range(256)) * 2048
print(len(data))

# Window by window: case maps, translate, hex, base64, codecs, tabs.
h = data.hex()
print(len(h), h[:12], h[-12:], data.hex(':', 3)[-14:], data[:9].hex('-', -4))
print(bytes.fromhex(h) == data, bytearray.fromhex(' ' + h[:999] + 'f ') == data[:500])
print(data.upper()[-30:], data.title()[90:110], data.swapcase()[60:70])
print(data.translate(bytes.maketrans(b'abc', b'xyz'), b'\x00\xff')[-5:])
tabs = b'alpha\tbe\t\tta\r\n\tgamma ' * 20_000
print(len(tabs.expandtabs(4)), tabs.expandtabs(4)[:40])
u = 'héllo wörld ☃ 𝄞 ' * 20_000
e = u.encode()
print(len(e), e.decode() == u, u.encode('utf-16').decode('utf-16') == u)
print(u.encode('latin-1', 'replace')[:20], len(u.encode('ascii', 'xmlcharrefreplace')))
bad = b'ok \xff\xe2\x98 ' * 30_000
print(len(bad.decode('utf-8', 'replace')), bad.decode('utf-8', 'backslashreplace')[-16:])
try:
    (e + b'\xff' + e).decode()
except UnicodeDecodeError as x:
    print(x)
try:
    (u + '☃x').encode('latin-1')
except UnicodeEncodeError as x:
    print(x)
b64 = base64.b64encode(data)
print(len(b64), b64[-12:], base64.b64decode(b64) == data)
print(binascii.unhexlify(binascii.hexlify(data)) == data)
print(binascii.a2b_base64(b64[:-1] + b'!' * 10 + b64[-1:]) == data)

# Pieces and matches, a unit each.
csv = b'k,v,' * 100_000
parts = csv.split(b',')
print(len(parts), parts[:3], parts[-2:], len(csv.rsplit(b',', 50_000)))
words = b' ab  cd\tef ' * 50_000
print(len(words.split()), words.rsplit(None, 2)[:2], len(words.splitlines()))
lines = b'x\r\ny\rz\n' * 50_000
print(len(lines.splitlines()), lines.splitlines(True)[:3])
print(csv.count(b','), csv.find(b'v,k', 300_000), csv.rindex(b'k,v', 0, 200_001))
r = csv.replace(b',', b';;')
print(len(r), r[:10], bytearray(csv).replace(b'k,', b'', 70_000)[-12:])
print(len(b'-'.join([b'ab', bytearray(b'c')] * 100_000)))
j = bytearray(b'-').join([bytearray(b'%d' % i) for i in range(150_000)])
print(type(j).__name__, len(j), j[:12], j[-14:], hashlib.sha256(bytes(j)).hexdigest()[:16])
print(b'v,k,' in csv, 44 in csv, b'zz' not in csv)
print([len(p) for p in csv.partition(b'v,k,v,k')], [len(p) for p in csv.rpartition(b'k,v,k')])

# Scans to one value, and output squeezed a block at a time.
big = data * 10
print(binascii.crc32(big), binascii.crc_hqx(big[:1_500_000], 7), hashlib.sha256(big).hexdigest()[:16])
print(hashlib.shake_128(b'p2').digest(3_000_000)[-8:].hex())
print(big[1::3][-6:], len(big[::-2]))
print(len(math.prod([b'ab', 300_000, 2])))
ba = bytearray(data)
ba *= 3
print(len(ba), ba[-3:])

# Lists and tuples of ints, read in place a step at a time.
ints = list(range(256)) * 1_000
print(len(bytes(ints)), bytes(ints)[-3:], bytearray(tuple(ints))[250:260])
ba = bytearray(b'ab')
ba.extend(ints)
print(len(ba), ba[-3:])
ba[1:2] = ints
print(len(ba), ba[:4])

# A full bytearray past 8 MiB grows into a new block a step at a time, then
# does the operation that grew it.
full = data * 17
def grown(op):
    g = bytearray(full)
    r = op(g)
    return len(g), g[:4], g[-4:], r is g
print(grown(lambda g: g.append(7)), grown(lambda g: g.insert(3, 7)))
print(grown(lambda g: g.extend(b'\x07' * 9)), grown(lambda g: g.extend([7, 7])))
def iadd(g):
    h = g
    h += b'xy'
    return h
print(grown(iadd), grown(lambda g: g.__setitem__(slice(2, 3), b'\x07\x07')))

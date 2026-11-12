# str.translate, str.maketrans and str.format_map; bytes.translate,
# bytes.maketrans and bytes.expandtabs; float.hex and float.fromhex, with
# CPython's errors and rounding (docs/spec.md §12). Builtin dict tables run
# natively; other mappings and values go through the prelude twin.
def show(f, *a, **k):
    try:
        print(repr(f(*a, **k)))
    except BaseException as e:
        print(type(e).__name__, e)
t = str.maketrans('abc', 'xyz', 'd')
print(sorted(t.items()))
show('abcdabcd'.translate, t)
show('héllo wörld ✓'.translate, {ord('é'): 'e', ord('ö'): None, ord('✓'): 0x2717, 32: '__'})
show('abc'.translate, {97: True, 98: 1.5})
show('abc'.translate, {97: -1})
show('abc'.translate, {97: 0x110000})
show('abc'.translate, 'XYZ' * 40)
show('abc'.translate, ['A'] * 98)
class M(dict):
    def __missing__(self, k): return chr(k).upper()
show('abc'.translate, M({98: 'B!'}))
class G:
    def __getitem__(self, k):
        if k == 99: raise LookupError
        return k + 1
show('abc'.translate, G())
show('abc'.translate, 5)
show(str.maketrans, {'a': 'b', 98: None})
show(str.maketrans, {'ab': 'b'})
show(str.maketrans, {1.5: 'b'})
show(str.maketrans, 'ab')
show(str.maketrans, 'ab', 'c')
show(str.maketrans, 'ab', 3)
show(str.maketrans, 3, 'ab')
show(str.maketrans, 'ab', 'cd', 5)
show(''.maketrans, 'a', 'b')
show('{a} {b[0]} {c.real}'.format_map, {'a': 1, 'b': [2], 'c': 3})
class D(dict):
    def __missing__(self, k): return '<' + k + '>'
show('{x} {y!r:>5}'.format_map, D(x=1))
show('{0}'.format_map, {})
show('{}'.format_map, {})
show('{z}'.format_map, {})
show('{z}'.format_map, 5)
show('x'.format_map)
for x in (0.0, -0.0, 1.0, -1.5, 0.1, 1e308, 5e-324, 2.2250738585072014e-308, 2.225073858507201e-308, float('inf'), float('-inf'), float('nan'), 3.141592653589793, 1/3):
    h = x.hex()
    print(h, float.fromhex(h) == x or x != x)
for s in ('0x1p0', ' 0X1.8P+1 ', '-0x.8p1', '1', 'a', 'ff.f', '0x1p-1074', '0x1p-1075', '0x1.8p-1074', '0x3p-1076', '0x1p1023', '0x1.fffffffffffffp1023', '0x1.fffffffffffff8p1023', '0x2p1023', 'inf', '-Infinity', 'nAn', '0x', '0x.p1', '0x1p', '0x1p+', '1e5', '0x1.00000000000008p0', '0x1.00000000000018p0', '0x1.000000000000081p0', '0x0p99999999999999999999', '0x1p-99999999999999999999', '0x1p99999999999999999999', '\u20030x1p0', '0x1_0p0', '.8', '8.'):
    show(float.fromhex, s)
class F(float): pass
show(F.fromhex, '0x1p1'); print(type(F.fromhex('0x1p1')).__name__)
show(float.fromhex, 5)
show((2.5).hex); show(F(2.5).hex)
show(b'abcabc'.translate, bytes.maketrans(b'abc', b'xyz'))
show(b'abcabc'.translate, None, b'b')
show(b'abcabc'.translate, None, delete=b'ac')
show(b'abc'.translate, b'x')
show(b'abc'.translate, 'x' * 256)
show(b'abc'.translate, None, 'a')
show(bytes.maketrans, b'ab', b'c')
show(bytes.maketrans, 'ab', b'cd')
print(len(bytes.maketrans(b'', b'')), bytes.maketrans(b'', b'')[:5])
show(b'a\tbc\td\n\te\r\tf'.expandtabs)
show(b'a\tbc\td'.expandtabs, 3)
show(b'a\tbc\td'.expandtabs, tabsize=0)
show(b'a\tbc\td'.expandtabs, -2)
show(b'a\tb'.expandtabs, 2**40)
show(b''.expandtabs)
print(b'xyz'.translate(None) is not None, 'translate' in dir(''), 'hex' in dir(1.0), 'expandtabs' in dir(b''))
class B(bytes): pass
print(type(B.fromhex('0aff')).__name__, B.fromhex('0aff'), type(B(b'x').fromhex('00')).__name__)
print(type(F(1.0).fromhex('0x1p3')).__name__, F(1.0).fromhex('0x1p3'))
class S(str): pass
print(S.maketrans('a', 'b'), S('abc').translate({97: 'q'}), type(S('abc').translate({})).__name__)
print(float.fromhex.__name__ if hasattr(float.fromhex, '__name__') else '')

# fromhex rounds half to even into 53 bits, subnormals included: random
# strings near the boundaries must match CPython bit for bit.
import hashlib
import random
rng = random.Random(7)
hs = []
for _ in range(300):
    ds = ''.join(rng.choice('0123456789abcdef') for _ in range(rng.randint(1, 18)))
    k = rng.randint(0, len(ds))
    e = rng.choice((rng.randint(-1100, -1000), rng.randint(1000, 1030), rng.randint(-60, 60)))
    s = '0x' + ds[:k] + '.' + ds[k:] + 'p' + str(e)
    try:
        h = float.fromhex(s).hex()
    except OverflowError:
        h = 'overflow'
    hs.append(h)
print(hs[:5], hashlib.sha256('\n'.join(hs).encode()).hexdigest())
ba = bytearray(b'a\tbc\tabc')
for r in (ba.translate(bytes.maketrans(b'abc', b'xyz')), ba.translate(None, b'b'), ba.translate(None),
          ba.expandtabs(4), bytearray.maketrans(b'a', b'b')[97:99], bytearray.fromhex('41 42'), ba.fromhex('43')):
    print(type(r).__name__, r)
print(ba.translate(None) is ba, b'ab'.translate(bytearray(range(256))), b'ab'.translate(None, bytearray(b'a')))

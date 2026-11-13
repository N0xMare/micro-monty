# Items 20 and 21 (and K1's shapes): arguments are converted in CPython's
# order and stop at the first bad one: math's functions of reals (hypot,
# fma, dist p[i] then q[i], log, atan2, ...; ldexp's exponent only an
# int), complex (real's __complex__, both type checks, then __float__),
# range, a slice's step before its start and stop, round's number before
# ndigits, list.index's bounds, int(x, base)'s base. A native converts
# only the arguments it needs as ints (list.insert keeps its item). None
# is not an absent maxsplit, count, tabsize, fill character or hex
# separator; byte items take __index__ in bytes, bytearray, extend and
# int.from_bytes. (CPython 3.14's DeprecationWarnings for a complex
# `imag` or a `__complex__` `real` beside one are not emitted by mm, and
# ignored here.)
import warnings
warnings.simplefilter('ignore', DeprecationWarning)
log = []
class Fl:
    def __init__(self, v): self.v = v
    def __float__(self): log.append(('float', self.v)); return float(self.v)
class Ix:
    def __init__(self, v): self.v = v
    def __index__(self): log.append(('index', self.v)); return self.v
class Cx:
    def __init__(self, v): self.v = v
    def __complex__(self): log.append(('complex', self.v)); return complex(self.v)
class Obj: pass
def t(name, f):
    log.clear()
    try:
        r = f()
    except Exception as e:
        r = f'{type(e).__name__}: {e}'
    print(name, repr(r), log)
for a in (Fl(1), Ix(2), Cx(3), 'x', Obj(), 1.5, None, '1'):
    for b in (Fl(4), Ix(5), Cx(6), 'y', Obj(), 2):
        t(f'complex({type(a).__name__}, {type(b).__name__})', lambda: complex(a, b))
        t(f'complex(imag={type(b).__name__})', lambda: complex(imag=b))
        t(f'complex(real={type(a).__name__}, imag={type(b).__name__})', lambda: complex(real=a, imag=b))
import math
for a in (Fl(1), Ix(2), 'x', Obj(), 1.5):
    for b in (Fl(4), Ix(5), 'y', Obj(), 2):
        t(f'fma {type(a).__name__} {type(b).__name__}', lambda: math.fma(a, b, Fl(7)))
        t(f'hypot {type(a).__name__} {type(b).__name__}', lambda: math.hypot(a, b, Fl(7)))
        t(f'hypot* {type(a).__name__} {type(b).__name__}', lambda: math.hypot(*([a, b, Fl(7)] * 3000)))
        t(f'dist {type(a).__name__} {type(b).__name__}', lambda: math.dist([a, Fl(8)], [b, Fl(9)]))
        t(f'dist2 {type(a).__name__} {type(b).__name__}', lambda: math.dist((Fl(8), a), (Fl(9), b)))
        t(f'dist3 {type(a).__name__} {type(b).__name__}', lambda: math.dist(iter([a, Fl(8)]), [b]))
        t(f'log {type(a).__name__} {type(b).__name__}', lambda: math.log(a, b))
        t(f'atan2 {type(a).__name__} {type(b).__name__}', lambda: math.atan2(a, b))
        t(f'copysign {type(a).__name__} {type(b).__name__}', lambda: math.copysign(a, b))
        t(f'ldexp {type(a).__name__} {type(b).__name__}', lambda: math.ldexp(a, b))
        t(f'isclose {type(a).__name__} {type(b).__name__}', lambda: math.isclose(a, b))
        t(f'pow {type(a).__name__} {type(b).__name__}', lambda: math.pow(a, b))
        t(f'nextafter {type(a).__name__} {type(b).__name__}', lambda: math.nextafter(a, b))
        t(f'remainder {type(a).__name__} {type(b).__name__}', lambda: math.remainder(a, b))
        t(f'fmod {type(a).__name__} {type(b).__name__}', lambda: math.fmod(a, b))
        t(f'range {type(a).__name__} {type(b).__name__}', lambda: range(a, b, Ix(9)))
        t(f'range2 {type(a).__name__} {type(b).__name__}', lambda: range(Ix(9), a, b))
        t(f'slice {type(a).__name__} {type(b).__name__}', lambda: [1, 2, 3][a:b:Ix(1)])
        t(f'slice2 {type(a).__name__} {type(b).__name__}', lambda: 'abc'[Ix(1):a:b])
        t(f'slice3 {type(a).__name__} {type(b).__name__}', lambda: (1, 2)[a:Ix(1):b])
        t(f'sliceset {type(a).__name__} {type(b).__name__}', lambda: [1, 2, 3].__setitem__(slice(a, b, Ix(1)), [4]))
        t(f'slicedel {type(a).__name__} {type(b).__name__}', lambda: [1, 2, 3].__delitem__(slice(Ix(1), a, b)))
        t(f'islice {type(a).__name__} {type(b).__name__}', lambda: list(__import__('itertools').islice(range(9), Ix(1), a, b)))
        t(f'pow3 {type(a).__name__} {type(b).__name__}', lambda: pow(Ix(3), a, b))
        t(f'divmod {type(a).__name__} {type(b).__name__}', lambda: divmod(a, b))
        t(f'round {type(a).__name__} {type(b).__name__}', lambda: round(a, b))
        t(f'int {type(a).__name__} {type(b).__name__}', lambda: int(a, b))
        t(f'comb {type(a).__name__} {type(b).__name__}', lambda: math.comb(a, b))
        t(f'perm {type(a).__name__} {type(b).__name__}', lambda: math.perm(a, b))
        t(f'gcd {type(a).__name__} {type(b).__name__}', lambda: math.gcd(a, b, Ix(9)))
        t(f'lcm {type(a).__name__} {type(b).__name__}', lambda: math.lcm(Ix(9), a, b))
        t(f'isqrt {type(a).__name__}', lambda: math.isqrt(a))
        t(f'insert {type(a).__name__}', lambda: [1].insert(a, b))
        t(f'list.index {type(a).__name__} {type(b).__name__}', lambda: [1, 2, 3, 4].index(3, a, b))
        t(f'str.find {type(a).__name__} {type(b).__name__}', lambda: 'abcd'.find('c', a, b))
        t(f'to_bytes {type(a).__name__} {type(b).__name__}', lambda: (5).to_bytes(a, 'big'))
        t(f'bytes.center {type(a).__name__} {type(b).__name__}', lambda: b'a'.center(a, b))
        t(f'str.center {type(a).__name__}', lambda: 'a'.center(a, b))
        t(f'zfill {type(a).__name__}', lambda: 'a'.zfill(a))
        t(f'enumerate {type(a).__name__}', lambda: list(enumerate('ab', a)))

for s in ('a b\tc', b'a b\tc', bytearray(b'a b\tc')):
    k = type(s).__name__
    sp = ' ' if isinstance(s, str) else b' '
    a = 'a' if isinstance(s, str) else b'a'
    for v in (None, 2, 2.0, 'x', b'x', True):
        t(f'{k} split {v!r}', lambda: s.split(None, v))
        t(f'{k} split kw {v!r}', lambda: s.split(maxsplit=v))
        t(f'{k} rsplit kw {v!r}', lambda: s.rsplit(sp, maxsplit=v))
        t(f'{k} replace {v!r}', lambda: s.replace(a, a + a, v))
        t(f'{k} replace kw {v!r}', lambda: s.replace(a, a, count=v))
        t(f'{k} expandtabs {v!r}', lambda: s.expandtabs(v))
        t(f'{k} expandtabs kw {v!r}', lambda: s.expandtabs(tabsize=v))
        t(f'{k} ljust {v!r}', lambda: s.ljust(7, v))
        t(f'{k} rjust {v!r}', lambda: s.rjust(7, v))
        t(f'{k} center {v!r}', lambda: s.center(7, v))
        t(f'{k} zfill {v!r}', lambda: s.zfill(v))
        t(f'{k} splitlines {v!r}', lambda: s.splitlines(v))
        t(f'{k} strip {v!r}', lambda: s.strip(v))
        t(f'{k} find {v!r}', lambda: s.find(a, v))
        t(f'{k} count {v!r}', lambda: s.count(a, None, v))
        t(f'{k} startswith {v!r}', lambda: s.startswith(a, v))
        if not isinstance(s, str):
            t(f'{k} hex {v!r}', lambda: s.hex(v))
            t(f'{k} hex2 {v!r}', lambda: s.hex(':', v))
            t(f'{k} hex kw {v!r}', lambda: s.hex(sep=v))
            t(f'{k} hex kw2 {v!r}', lambda: s.hex(bytes_per_sep=v))
            t(f'{k} translate {v!r}', lambda: s.translate(None, v))
for v in (None, 2, 'x'):
    t(f'bytes.fromhex {v!r}', lambda: bytes.fromhex(v))
    t(f'str.zfill {v!r}', lambda: 'ab'.zfill(v))
    t(f'encode {v!r}', lambda: 'ab'.encode(v))
    t(f'encode2 {v!r}', lambda: 'ab'.encode('utf-8', v))
    t(f'decode {v!r}', lambda: b'ab'.decode(v))
    t(f'decode2 {v!r}', lambda: b'ab'.decode('utf-8', v))
    t(f'partition {v!r}', lambda: 'ab'.partition(v))
    t(f'join {v!r}', lambda: 'ab'.join(v))
    t(f'removeprefix {v!r}', lambda: 'ab'.removeprefix(v))
    t(f'index {v!r}', lambda: 'ab'.index('a', v, v))


class Ixr:
    def __init__(self, v): self.v = v
    def __index__(self): log.append(('index', self.v)); return self.v
    def __repr__(self): return f'Ixr({self.v})'
l = [1]; l.insert(Ixr(0), Ixr(5)); print(l, log)
x = Ixr(3); print([1, x].index(x, Ixr(0)))
b = bytearray(b'ab'); b.insert(Ixr(0), Ixr(65)); print(b)
t('bytes', lambda: bytes([Ix(65), 66]))
t('bytes bad', lambda: bytes([Ix(65), 'x', Ix(66)]))
t('bytes range', lambda: bytes([Ix(65), Ix(300), Ix(66)]))
t('bytearray tuple', lambda: bytearray((Ix(65), 66, Ix(67))))
t('bytes gen', lambda: bytes(Ix(i) for i in (65, 66)))
t('ba extend', lambda: (lambda b: (b.extend([Ix(67)]), b)[1])(bytearray(b'z')))
t('ba extend tuple', lambda: (lambda b: (b.extend((Ix(1), 2)), b)[1])(bytearray(b'z')))
t('ba extend bad', lambda: (lambda b: (b.extend([Ix(1), 300]), b)[1])(bytearray(b'z')))
t('list extend', lambda: (lambda l: (l.extend([Ix(1), 2]), [type(v).__name__ for v in l])[1])([0]))
t('from_bytes', lambda: int.from_bytes([Ix(200), 1], 'big'))
t('from_bytes tuple', lambda: int.from_bytes((Ix(1), Ix(2)), 'little'))
t('from_bytes signed', lambda: int.from_bytes([Ix(255)], 'big', signed=True))
t('bytes count', lambda: bytes(Ix(3)))
t('ba setitem', lambda: (lambda b: (b.__setitem__(slice(0, 1), [Ix(70)]), b)[1])(bytearray(b'ab')))
t('bytes join', lambda: b''.join([bytes([Ix(65)])]))
t('complex imag only', lambda: complex(imag=Ix(5)))
t('int base kw', lambda: int('11', base=Ix(2)))
t('split kw', lambda: 'a b c'.split(maxsplit=Ix(1)))

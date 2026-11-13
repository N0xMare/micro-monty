# Item 14's arguments (correctness review, K2): an int subclass is its
# value as an index (`l[v] = x`, `del l[v]`, `ba[i] = v`), a byte
# (`ba.append`, `v in bytes`, `bytes.find`), `hex`'s group size and a count
# in itertools, `deque(maxlen=)` and `random.choices(k=)`, never through
# its own `__int__` or `__index__`; `itertools.count` of one counts ints;
# a float subclass with `__index__` indexes; `math.floor`/`ceil` of a
# float subclass round its value; `str()` of a bytes subclass is its
# value's repr; `splitlines` reads `keepends`'s `__bool__`.
import math, itertools, random, collections
log = []
def t(name, f):
    log.clear()
    try:
        r = f()
    except Exception as e:
        r = f'{type(e).__name__}: {e}'
    print(name, repr(r), log)
class IS(int): pass
class FS(float):
    def __index__(self): log.append('index'); return 1
class OnlyInt:
    def __int__(self): log.append('int'); return 3
class LS(list):
    def __len__(self): log.append('len'); return list.__len__(self)
class DS(dict):
    def keys(self): log.append('keys'); return ['a']
    def __getitem__(self, k): log.append(('gi', k)); return 9
class BS(bytes):
    def __repr__(self): return 'BS-repr'
class SR(str): pass
class R:
    def __rmul__(self, o): return 'rmul'
class SS(str): pass
class It:
    def __init__(self): self.n = 3
    def __iter__(self): return self
    def __next__(self):
        if self.n == 0: raise StopIteration
        self.n -= 1; return self.n
    def __length_hint__(self): log.append('hint'); return 3
# attributes
d = {}; 
def ior():
    d = {}
    d |= DS(b=1)
    return d
t("'ab'[FS]", lambda: 'ab'[FS(0.5)])
t('range(FS)', lambda: range(FS(0.5)))
t('str(BS)', lambda: str(BS(b'x')))
t('SR*R', lambda: SR('ab') * R())
t('l[IS]=', lambda: (lambda l: (l.__setitem__(IS(0), 9), l)[1])([1, 2]))
def setit():
    l = [1, 2]; l[IS(1)] = 7; del l[IS(0)]; return l
t('l[IS] stmt', setit)
t('ba.append(IS)', lambda: (lambda b: (b.append(IS(65)), b)[1])(bytearray()))
def basetit():
    b = bytearray(b'ab'); b[IS(0)] = IS(66); return b
t('ba[IS]=IS', basetit)
t('IS in bytes', lambda: IS(97) in b'abc')
t('bytes.find(IS)', lambda: b'abc'.find(IS(98)))
t('bytes.hex(sep, IS)', lambda: b'abc'.hex(':', IS(2)))
class FI(float):
    def __float__(self): log.append('float'); return 2.5
t('math.floor(FI)', lambda: math.floor(FI(1.5)))
t('math.ceil(FI)', lambda: math.ceil(FI(1.5)))
class II(int):
    def __int__(self): log.append('int'); return 7
    def __index__(self): log.append('index'); return int.__index__(self)
t('repeat(II)', lambda: list(itertools.repeat(1, II(2))))
t('combinations(II)', lambda: list(itertools.combinations('abc', II(2))))
t('permutations(II)', lambda: list(itertools.permutations('ab', II(2))))
t('product(II)', lambda: len(list(itertools.product('ab', repeat=II(2)))))
t('batched(II)', lambda: list(itertools.batched('abcd', II(2))))
t('deque(maxlen=II)', lambda: collections.deque([1, 2, 3], maxlen=II(2)))
t('choices(k=II)', lambda: len(random.choices('ab', k=II(2))))
t('count(II)', lambda: type(next(itertools.count(II(2)))).__name__)
class BB(int):
    def __bool__(self): log.append('bool'); return True
t('splitlines(BB)', lambda: 'a\nb'.splitlines(BB(0)))
class HN:
    def __iter__(self): return iter([1])
    def __length_hint__(self): return -1

class BB2(int):
    def __bool__(self): log.append('bool'); return True
class L:
    def __len__(self): log.append('len'); return 0
for s in ('a\nb', b'a\nb', bytearray(b'a\nb')):
    t('splitlines ' + type(s).__name__, lambda: (s.splitlines(BB2(0)), s.splitlines(keepends=L()), s.splitlines(True)))
class BSr(bytes):
    def __repr__(self): return 'BSr-repr'
b = BSr(b'x')
t('bytes subclass str', lambda: (str(b), f'{b}', format(b), '%s' % (b,), repr(b), [b]))
class IE(int):
    def __index__(self): log.append('IE index'); return 0
    def __int__(self): log.append('IE int'); return 0
t('list get/set/del IE', lambda: (lambda l: (l[IE(1)], l.__setitem__(IE(2), 'z'), l.pop(IE(0)), l)[3])(['a', 'b', 'c']))
t('range/tuple/str IE', lambda: (range(5)[IE(3)], (1, 2, 3)[IE(1)], 'abc'[IE(2)], b'abc'[IE(0)]))
t('ba insert IE', lambda: (lambda b: (b.insert(IE(1), IE(66)), b)[1])(bytearray(b'ac')))
t('islice IE', lambda: list(itertools.islice('abcdef', IE(1), IE(5), IE(2))))

# difffuzz divergence, seed 20492
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1a9b'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᪛'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#b' | mm: EXC ValueError invalid literal for int() with base #: '᪛'
import math
try:
    v1 = {42, 42, 5, 1, 1}
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v3 = '3.5'.strip().replace(repr([str(q) for q in [255, 1, 1000]]), 'AbC')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n9 = 1
    while n9 > 0:
        class Err10(ValueError):
                self.code = code
        try:
            try:
                raise Err10((chr(7863) * (2 ** 64 % 4)), 2)
            except Err10 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
            def __init__(self, v, w=0):
                self.w = w
        async def async_cleanup12():
            class CM:
                async def __aenter__(self):
                    return 3
            async def values():
                try:
                        yield i
                finally:
                    log.append("closed")
            try:
                        async for item in stream:
                                continue
                        else:
                            log.append("exhausted")
            except ValueError:
                log.append("caught")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
        class C25:
            tag = '\u00df'
            def __init__(self, v, w=0):
                self.v = v
                self.w = w
            def __repr__(self):
                return f'C25({self.v!r}, {self.w})'
            def __lt__(self, other):
                return (self.v, self.w) < (other.v, other.w)
            def __add__(self, other):
                return C25(self.v + (other.v if isinstance(other, C25) else other), self.w)
            def __len__(self):
                return abs(self.w) % 5
            def __getitem__(self, i):
                return [self.v, self.w][i]
            def double(self):
                return self.v * 2
            def make(n):
                return C25(n, -n)
            @classmethod
            def named(cls):
                return cls.__name__
        objs = [C25(('caf\u00e9' * (pow(min(lst0), 14, 2 ** 61 - 1) % 4)).count(str({})), 1), C25(255, 4), C25(abs((v3.partition(',')[1].rfind(v3.ljust(3, chr(45))) >> (math.trunc(abs(lst0[n9]) % 30) % 9))).bit_length(), 2), C25.make(4)]
        a, b = objs[0], objs[1]
        def make26(start):
            total = start
            def step(by=1):
                nonlocal total
            return step
        s = make26(len(v1))
        fs = [lambda q, i=i: q * i for i in range(1)]
        try:
            raise ValueError(v3.replace(v3.zfill(6), v3.partition(',')[1]).lstrip('{}-{!r}-{:>4}'.format(lst0[n9], v3.split(' ')[2], (n9 >> (n9 % 9)))).swapcase())
        finally:
            print("finally", ((max(sorted({42, 5})) << (((1 << 100) - 1 if any([1, 0, -17, -17, 7]) else 10 ** 30 + 7) % 70)) * divmod(dict(sorted({'k0': 100, 'k1': -1}.items(), key=lambda kv: kv[1]))['k1'], int(chr(6811), 10))[0]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

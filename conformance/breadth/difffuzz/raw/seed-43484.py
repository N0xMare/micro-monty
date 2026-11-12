# difffuzz divergence, seed 43484
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u08e2'
#   mm:      EXC ValueError invalid literal for int() with base 10: '࣢'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '࣢'
import math
try:
    def f12(p0, p1):
        def f13(p0, *rest, kw=5):
            class Ctx14:
                def __init__(self, name):
                    self.name = name
        async def async_cleanup15():
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
                                raise ValueError("stop")
                        else:
                            log.append("exhausted")
            except ValueError:
                log.append("caught")
    print("f12(pow(pow(int(chr(2274)), 39, -5), 27, -5), (math.trunc(abs(((1 >> (42 % 9)) >> ((7 if True else 5) % 9))) % 30) << ((len({str(q): q for q in [2, 10]}) if ('%d' % (0.1,) in [str(q) for q in [255, 100, 0, 1, 7]]) else (sum([1000, 3]) << (math.gcd(abs(1000) % 30) % 70))) % 70)))", f12(pow(pow(int(chr(2274)), 39, -5), 27, -5), (math.trunc(abs(((1 >> (42 % 9)) >> ((7 if True else 5) % 9))) % 30) << ((len({str(q): q for q in [2, 10]}) if ('%d' % (0.1,) in [str(q) for q in [255, 100, 0, 1, 7]]) else (sum([1000, 3]) << (math.gcd(abs(1000) % 30) % 70))) % 70))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

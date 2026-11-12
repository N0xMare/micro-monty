# difffuzz divergence, seed 27392
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2efa'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⻺'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#efa' | mm: EXC ValueError invalid literal for int() with base #: '⻺'
import math
try:
    def f17(p0, p1):
        if bool(1e300):
            if (w := sum([ord(c) for c in '%x' % (p0,).swapcase()])) > 27:
                print("walrus", w)
            match (-3, 1000, -17, ):
                case {'k0': v, **others}:
                    print("map", v, others)
            class C19:
                def __init__(self, v, w=0):
                    self.w = w
            async def async_cleanup21():
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
                                    return log
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
    print("f17(f'{None!r}{round((-2.25 ** 0.5))=}|', (int(chr(12026)) ^ n1))", f17(f'{None!r}{round((-2.25 ** 0.5))=}|', (int(chr(12026)) ^ n1)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 29932
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u16f9'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᛹'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#' | mm: EXC ValueError invalid literal for int() with base #: '᛹'
import math
try:
    if isinstance([(sum([5, 0]) + i1) for i1 in range(1) if i1 % 3], str):
        def rec2(n):
            if n <= 1:
                return n
    else:
        class C3:
            def __init__(self, v, w=0):
                self.w = w
        objs = [C3(int(chr(5881)), 2), C3(int(abs((round(0.1, 1) - 3.0))), 2), C3(list(filter(lambda q: q > 4, [i + v for i, v in enumerate(([7, 255, 255, 0, 2] + [0, 100, 7, 1000, 2]), 0)]))[round(3.0)], 1), C3.make(2)]
        async def async_cleanup4():
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
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 4014
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u0b81'
#   mm:      EXC ValueError invalid literal for int() with base 2: '஁'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '஁'
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f18(p0, *rest, kw=4):
        class Err19(KeyError):
            def __init__(self, msg, code=0):
                self.code = code
        try:
            try:
                raise Err19(repr((10, 5, 255, )).title().split()[1].title(), 3)
            except Err19 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
        *a, b = ('-42' * (-1 % 4)).ljust(1, chr(45)).rjust(7, chr(45)).ljust(1, chr(42))
        import asyncio
        from contextlib import aclosing
        async def async_cleanup21():
            log = []
            class CM:
                async def __aenter__(self):
                    return 3
                async def __aexit__(self, kind, value, tb):
                    return False
            async def values():
                try:
                    for i in range(3):
                        yield i
                finally:
                    log.append("closed")
            try:
                async with CM() as offset:
                    async with aclosing(values()) as stream:
                        async for item in stream:
                                break
                        else:
                            log.append("exhausted")
            except ValueError:
                log.append("caught")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v23 = f18(sorted({s24 % 2 for s24 in [-3, 7, -3, 2]})[:-2:-2])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n25 = 3
    while n25 > 0:
        if False:
            def rec26(n):
                if n <= 1:
                    return n
                    if k % 2 == 0:
                        yield k * -3
        else:
            class C32:
                def __init__(self, v, w=0):
                    self.w = w
                def __repr__(self):
                    return f'C32({self.v!r}, {self.w})'
            objs = [C32(v23, 3), C32(int(chr(2945), 2), -1), C32(sum(list(filter(lambda q: q > 1, [a * b for a, b in zip([a * b for a, b in zip(lst0, [-3, 10])], (v27 + v12))]))), 3), C32.make(3)]
            def risky():
                try:
                    print([1, 2].remove(math.isqrt(abs(sum((sorted({1000, -1, -1}) * 0))) % 30)))
                except ValueError as e:
                    print("caught", type(e).__name__, e, e.args)
            try:
                raise ValueError(f'{[(n8 >> (0 % 9)), int(-0.0), min([7, 0, 3, 0, 3])]!r}|')
            except LookupError as e:
                print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 11687
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u05cf'
#   mm:      EXC ValueError invalid literal for int() with base 10: '׏'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cf' | mm: EXC ValueError invalid literal for int() with base #: '׏'
import math
try:
    if bool({'k0': 1}):
        print(isinstance(int(chr(1487)), int), end='-42')
        def f2(p0):
            async def async_cleanup3():
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
                                    break
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
            total = start
            def step(by=1):
                return total
                def __init__(self, v, w=0):
                    self.w = w
            match None:
                case [] | ():
                    print("empty")
            class Err10(LookupError):
                    self.code = code
            try:
                try:
                    raise Err10(f'{abs(1e300):g} and {(n7 / 255):+.2f} {None!r:>12}|'[-4:6], 8)
                except Err10 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

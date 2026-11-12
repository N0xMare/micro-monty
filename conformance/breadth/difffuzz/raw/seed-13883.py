# difffuzz divergence, seed 13883
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: Err14 1267650600228229401496703205375-'\u2d6b'-   1 9 wrapped
#   mm:      Err14 1267650600228229401496703205375-'⵫'-   1 9 wrapped
# signature: CPython: Err# #-'\u#d#b'-   # # wrapped | mm: Err# #-'⵫'-   # # wrapped
import math
try:
    n9 = 1
    while n9 > 0:
        async def async_cleanup10():
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
        print(*list(reversed([((len((42, )) << (''.index('mississippi') % 70)) + i11) for i11 in range(4) if i11 % 2])), *'123'[-2:-6], sep=",")
        def make12(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
    class Err14(LookupError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err14('{}-{!r}-{:>4}'.format((1 << 100) - 1, chr(11627), n9), 9)
        except Err14 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)

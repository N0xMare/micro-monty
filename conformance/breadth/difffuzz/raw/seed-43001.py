# difffuzz divergence, seed 43001
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err17 ['\u20f1'] 0 wrapped
#   mm:      Err17 ['⃱'] 0 wrapped
# signature: CPython: Err# ['\u#f#'] # wrapped | mm: Err# ['⃱'] # wrapped
import math
try:
    for i9 in range(6):
        if i9 == 5:
            continue
        def rec10(n):
            if n <= 1:
                return n
        match (tuple(sorted(({'k0': -3, 'k1': 0, 'k2': 100} | {'k0': -1, 'k1': 0}).items()))[-1:] + (-17, )):
            case list(l):
                print("list", len(l))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f15(p0, p1):
        import asyncio
        from contextlib import aclosing
        async def async_cleanup16():
            log = []
            class CM:
                async def __aenter__(self):
                    return 3
                async def __aexit__(self, kind, value, tb):
                    return False
            async def values():
                try:
                    for i in range(1):
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
        class Err17(ValueError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err17(repr(list(chr(8433))), 0)
            except Err17 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err26(Exception):
        def __init__(self, msg, code=0):
            self.code = code
    try:
        try:
            raise Err26(str((i9 in ())).replace(f15(sorted({42, 7, 100, -1}), [len(w) for w in []]), '-42'[8:-5].removeprefix('123'.join(['caf\u00e9', 'hello'])))[-5:1], 3)
        except Err26 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

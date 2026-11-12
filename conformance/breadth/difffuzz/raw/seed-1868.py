# difffuzz divergence, seed 1868
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v18 ['\u1bf6']
#   mm:      v18 ['᯶']
# signature: CPython: v# ['\u#bf#'] | mm: v# ['᯶']
import math
try:
    n3 = 4
    while n3 > 0:
        n3 -= 1
        match str(dict(zip('hello'[n3].splitlines(), [len(w) for w in ['tab\there']]))):
            case {'k0': v, **others}:
                print("map", v, others)
        def f5():
            match (1, "x"):
                case {}:
                    print("any mapping")
            import asyncio
            from contextlib import aclosing
            async def async_cleanup6():
                log = []
                class CM:
                    async def __aenter__(self):
                        return 3
                    async def __aexit__(self, kind, value, tb):
                        return False
                async def values():
                    try:
                        for i in range(4):
                            yield i
                    finally:
                        log.append("closed")
                try:
                    async with CM() as offset:
                        async with aclosing(values()) as stream:
                            async for item in stream:
                                    raise ValueError("stop")
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
            return n3
        lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v18 = list((chr(7158) + ('mississippi'.removesuffix('') * (f5() % 4))))
    print('v18', v18)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

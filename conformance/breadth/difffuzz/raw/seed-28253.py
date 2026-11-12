# difffuzz divergence, seed 28253
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v26 5.587935447692873e-309
#   mm:      v26 5.58793544769287e-309
# signature: CPython: v# #.#e-# | mm: v# #.#e-#
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i24 in [a * b for a, b in zip([255, 100, 100, -3, -3], list(({'k0': 10, 'k1': -17} | dict(zip(['3.5'], [1000, 5]))).values()))]:
        import asyncio
        from contextlib import aclosing
        async def async_cleanup25():
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
                                return log
                        else:
                            log.append("exhausted")
            except ValueError:
                log.append("caught")
        v26 = (sum(list(reversed(sorted(lst0, key=str)))) / (int(1e300) << (i24 % 70)))
        print('v26', v26)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

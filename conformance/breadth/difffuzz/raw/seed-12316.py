# difffuzz divergence, seed 12316
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['enter', 3, 4, 5, 'closed', ('exit', None)]
#   mm:      EXC MemoryError 
# signature: CPython: ['enter', #, #, #, 'closed', ('exit', None)] | mm: EXC MemoryError 
import math
try:
    if ('tab\there'.swapcase().swapcase() * ((2 ** 64 ** (max([-3, 5, -3]) % 7)) % 4)).isalpha():
        print(a, b)
    elif True:
        for i5 in range(5):
            import asyncio
            from contextlib import aclosing
            async def async_cleanup6():
                log = []
                class CM:
                    async def __aenter__(self):
                        log.append("enter")
                        return 3
                    async def __aexit__(self, kind, value, tb):
                        log.append(("exit", kind.__name__ if kind else None))
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
                                log.append(item + offset)
                                if item == 2:
                                    return log
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
            print(asyncio.run(async_cleanup6()))
            def f7(p0):
                return ((math.factorial(round(-2.25, 2)) >> ((sum([]) >> (-v2 % 9)) % 9)) << (2 ** 64 % 70))
        async def async_cleanup11():
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
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

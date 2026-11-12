# difffuzz divergence, seed 5525
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ["'\\u193d'"]
#   mm:      ["'᤽'"]
# signature: CPython: ["'\\u#d'"] | mm: ["'᤽'"]
import math
try:
        import asyncio
        async def async_cleanup8():
            log = []
            class CM:
                async def __aenter__(self):
                    return 3
                async def __aexit__(self, kind, value, tb):
                    return True
            async def values():
                try:
                        yield i
                finally:
                    log.append("closed")
            try:
                async with CM() as offset:
                        async for item in stream:
                                return log
                        else:
                            log.append("exhausted")
            except ValueError:
                log.append("caught")
        data = repr(chr(6461)).split(',')
        print(sorted(data, key=str, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

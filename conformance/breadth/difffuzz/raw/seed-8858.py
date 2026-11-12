# difffuzz divergence, seed 8858
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err13 '\u0890' 4 wrapped
#   mm:      Err13 '࢐' 4 wrapped
# signature: CPython: Err# '\u#' # wrapped | mm: Err# '࢐' # wrapped
import math
try:
    n11 = 1
    while n11 > 0:
        n11 -= 1
        import asyncio
        async def async_cleanup12():
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
                                continue
                        else:
                            log.append("exhausted")
            except ValueError:
                log.append("caught")
        class Err13(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err13(chr(2192), 4)
            except Err13 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

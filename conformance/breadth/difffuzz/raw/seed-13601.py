# difffuzz divergence, seed 13601
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['k1\u218ek0\u218ek2']
#   mm:      ['k1↎k0↎k2']
# signature: CPython: ['k#\u#ek#\u#ek#'] | mm: ['k#↎k#↎k#']
import math
try:
        class Ctx4:
            def __init__(self, name):
                self.name = name
            def __enter__(self):
                return self
            def __exit__(self, kind, value, tb):
                return True
        with Ctx4("a") as x, Ctx4("b"):
            print(1 / 0)
        v5 = chr(8590).join(list(dict(sorted({'k0': 1, 'k1': 0, 'k2': 3}.items(), key=lambda kv: kv[1])))).ljust(6, chr(45))
        class Ctx6:
            def __init__(self, name):
                self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    if (not all([])):
        print(lst0)
    else:
        data = v5.split()
        print(sorted(data, key=str, reverse=True))
        async def async_cleanup16():
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

# difffuzz divergence, seed 24943
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err21 '\u0fdc' 2 wrapped
#   mm:      Err21 '࿜' 2 wrapped
# signature: CPython: Err# '\u#fdc' # wrapped | mm: Err# '࿜' # wrapped
import math
try:
    for i3 in range(2, 7):
        async def async_cleanup4():
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
try:
    def f9():
        if (i3 < i3 <= sum([({'k0': 5, 'k1': 5, 'k2': 10}['k0'] + i10) for i10 in list(reversed(lst0)) if i10 % 2])):
            def rec11(n):
                if n <= 1:
                    return n
        else:
            lst0 = [3, 1, 2]
            import asyncio
            from contextlib import aclosing
            async def async_cleanup12():
                log = []
                class CM:
                    async def __aenter__(self):
                        return 3
                    async def __aexit__(self, kind, value, tb):
                        return False
                async def values():
                    try:
                        for i in range(2):
                            yield i
                    finally:
                        log.append("closed")
                try:
                    async with CM() as offset:
                        async with aclosing(values()) as stream:
                            async for item in stream:
                                    continue
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
        return sum(lst0)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v17 = {chr(8969): f9(), (f'{7} {i3:#b} '[:5] + f'{None!r:>12} and {1e300:.10g}'.partition(' ')[0]): int(1.5)}
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f20(p0, p1, p2, *rest, kw=1):
        class Err21(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err21(chr(4060), 2)
            except Err21 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
    print('f20(((list((v17 | v17).values()) + [ord(c) for c in \'123\'.center(0, chr(45))]) + list(reversed(list(dict(sorted(v17.items(), key=lambda kv: kv[1])).values())))), list(range(4, 1, 3)), f\'{[]!r} {(pow(5, 28, 2 ** 61 - 1) if ("123" in ["AbC", "mississippi", "a,b,,c"]) else int(0.1)):%}|{2.5:.2f}|\')', f20(((list((v17 | v17).values()) + [ord(c) for c in '123'.center(0, chr(45))]) + list(reversed(list(dict(sorted(v17.items(), key=lambda kv: kv[1])).values())))), list(range(4, 1, 3)), f'{[]!r} {(pow(5, 28, 2 ** 61 - 1) if ("123" in ["AbC", "mississippi", "a,b,,c"]) else int(0.1)):%}|{2.5:.2f}|'))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

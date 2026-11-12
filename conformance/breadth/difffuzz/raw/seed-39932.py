# difffuzz divergence, seed 39932
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: Err9 '\u0a53' 3 wrapped
#   mm:      Err9 '੓' 3 wrapped
# signature: CPython: Err# '\u#a#' # wrapped | mm: Err# '੓' # wrapped
import math
try:
    def f3(p0, p1, p2, *rest, kw=5):
        x = y = ([] + list(map(lambda q: q * -1 + 1, p1)))
        n7 = 1
        while n7 > 0:
            n7 -= 1
            class Ctx8:
                def __init__(self, name):
                    self.name = name
                def __enter__(self):
                    return self
                def __exit__(self, kind, value, tb):
                    return True
            with Ctx8("a") as x, Ctx8("b"):
                print(x.name * 2)
            class Err9(KeyError):
                def __init__(self, msg, code=0):
                    super().__init__(msg)
                    self.code = code
            try:
                try:
                    raise Err9((chr(2643).replace('AbC'.ljust(11, chr(45)), p0).lstrip(p2.join(['\U0001f600 ok', '\u00df'])[(n7 >> (n7 % 9))]) + p0), 3)
                except Err9 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f23(p0, p1):
        for i24 in sorted(frozenset(list({str(q): q for q in lst0}.values())) | {6}):
            async def async_cleanup25():
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
            for i26 in [ord(c) for c in ('caf\u00e9' + f'{1e-05:%} {p1:>8}|'.zfill(6))]:
                if i26 == 0:
                    continue
    print("f23(pow((100 | (f3('', [1000, 3, 100], '\\u00df') * (0 if False else 1))), 21, 7), ('hello'.lower().split('s')[2] + '{}-{!r}-{:>4}'.format(n13, '123', n18).zfill(4)).expandtabs())", f23(pow((100 | (f3('', [1000, 3, 100], '\u00df') * (0 if False else 1))), 21, 7), ('hello'.lower().split('s')[2] + '{}-{!r}-{:>4}'.format(n13, '123', n18).zfill(4)).expandtabs()))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 42694
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: 'x\u07b3\n\u07b3y'
#   mm:      EXC ValueError invalid literal for int() with base 10: 'x޳\n޳y'
# signature: CPython: EXC ValueError invalid literal for int() with base #: 'x\u#b#\n\u#b#y' | mm: EXC ValueError invalid literal for int() with base #: 'x޳\n޳y'
import math
try:
        v7 = {'caf\u00e9'.join(['a', '\u00df', '  pad  ']).partition(' ')[1].removesuffix(('hello' + '\U0001f600 ok').rjust(4, chr(42))): math.floor(math.pi)}
        async def async_cleanup8():
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
    class C22:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C22({self.v!r}, {self.w})'
    objs = [C22(math.factorial(abs((len((2, 7, -17, 5, )) ** (divmod(len(v7), ~10)[1] % 7))) % 30), 0), C22(int(chr(1971).join(list('x\ny')).split(',')[-1]), 3), C22(int('123'[divmod(i5, math.gcd(abs(i5) % 30))[0]]), -2), C22.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

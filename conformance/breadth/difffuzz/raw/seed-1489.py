# difffuzz divergence, seed 1489
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: 15511210043330985984000000
#   mm:      EXC MemoryError 
# signature: CPython: # | mm: EXC MemoryError 
import math
try:
    def f4(p0, p1, p2):
        def rec5(n):
            if n <= 1:
                return n
            return n * rec5(n - 1)
        print(rec5(25))
        async def async_cleanup6():
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
                                raise ValueError("stop")
                        else:
                            log.append("exhausted")
            except ValueError:
                log.append("caught")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n11 = 6
    while n11 > 0:
        n11 -= 1
        lst0 = [3, 1, 2]
        print(v3)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    try:
        print((math.factorial(abs((2 ** 64 ** ((n11 if False else n11) % 7))) % 30) >> (divmod('123'.partition(' ')[0].count('caf\u00e9'.expandtabs()), (f4('3.5', lst0, -1) if any(lst0) else int('a', 2)))[1] % 9)).nope)
    except LookupError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

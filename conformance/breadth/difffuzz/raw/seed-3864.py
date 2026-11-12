# difffuzz divergence, seed 3864
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u1af7'
#   mm:      EXC ValueError could not convert string to float: '᫷'
# signature: CPython: EXC ValueError could not convert string to float: '\u#af#' | mm: EXC ValueError could not convert string to float: '᫷'
import math
try:
    v5 = chr(6903)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    if ('tab\there' in list(''.replace('  pad  ', '-42').join(sorted(['AbC'], key=len)))):
        class Ctx14:
            def __init__(self, name):
                self.name = name
        def gen17(n):
                if k % 2 == 0:
                    yield k * 2
        try:
            print(float((v5 * (42 % 4))))
        except (TypeError, ValueError) as e:
            print("caught", type(e).__name__, e, e.args)
    else:
        def make19(start):
            total = start
            def step(by=1):
                return total
        s = make19(abs(int('%s|%r' % (float(v5), --2.25,), 2)).bit_length())
        def make20(start):
            total = start
            def step(by=1):
                return total
        async def async_cleanup21():
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
                                continue
                        else:
                            log.append("exhausted")
            except ValueError:
                log.append("caught")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 28280
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 3, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0ffc'
#   mm:      EXC ValueError invalid literal for int() with base 16: '࿼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ffc' | mm: EXC ValueError invalid literal for int() with base #: '࿼'
import math
try:
    n1 = 2
    while n1 > 0:
        n1 -= 1
        if True:
            def f3(p0, p1):
                return ((1 << 100) - 1 >> (int((abs(0.1) / math.tau)) % 9))
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
                                    break
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
        match [ord(c) for c in '{}-{!r}-{:>4}'.format(sum([2, 100, -3, 10]), ('a,b,,c' + '\U0001f600 ok').rjust(5, chr(42)), 7)]:
            case list(l):
                print("list", len(l))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    *a, b = '{}-{!r}-{:>4}'.format(int(float(n1)), 'mississippi'[-1:5].join('mississippi'.splitlines()), int(chr(4092), 16)).join(('\U0001f600 ok'.replace('mississippi', 'tab\there') * (sum([10]) % 4)).split(','))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

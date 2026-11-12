# difffuzz divergence, seed 37988
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1cca'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᳊'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cca' | mm: EXC ValueError invalid literal for int() with base #: '᳊'
import math
try:
    def f10(p0, p1, p2):
        def rec11(n):
            if n <= 1:
                return n
    print("f10(3 ** 50, int(chr(7370).expandtabs()), repr(float(int('caf\\u00e9', 16)))[min(list(map(lambda q: q * -1 + 1, list({'k0': 2}.values()))))])", f10(3 ** 50, int(chr(7370).expandtabs()), repr(float(int('caf\u00e9', 16)))[min(list(map(lambda q: q * -1 + 1, list({'k0': 2}.values()))))]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

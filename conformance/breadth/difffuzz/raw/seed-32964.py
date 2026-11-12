# difffuzz divergence, seed 32964
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u1ad9😀 ok'
#   mm:      EXC ValueError invalid literal for int() with base 2: '᫙😀 ok'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ad#😀 ok' | mm: EXC ValueError invalid literal for int() with base #: '᫙😀 ok'
import math
try:
    n26 = 1
    while n26 > 0:
        match (str(2.5).expandtabs() * (7 % 4)).strip():
            case [x]:
                print("one", x)
        v27 = int((chr(6873) + '\U0001f600 ok'[::1]), 2)
        match (1, "x"):
            case int(n) if n > 10:
                print("big", n)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

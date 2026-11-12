# difffuzz divergence, seed 11970
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u0ffc'
#   mm:      EXC ValueError invalid literal for int() with base 2: '࿼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ffc' | mm: EXC ValueError invalid literal for int() with base #: '࿼'
import math
try:
    n17 = 1
    while n17 > 0:
        n17 -= 1
        match dict(sorted(dict(sorted(dict(sorted(dict(sorted({}.items(), key=lambda kv: kv[1])).items(), key=lambda kv: kv[1])).items(), key=lambda kv: kv[1])).items(), key=lambda kv: kv[1])):
            case [x, y, *rest]:
                print("many", x, y, rest)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    lst0[3] = (((pow(0, 32, 1000) ** (pow(n17, 20, -5) % 7)) >> (int(chr(4092), 2) % 9)) ** ((math.trunc((0.1 ** 3)) ** (int(round(0.1, 2)) % 7)) % 7))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

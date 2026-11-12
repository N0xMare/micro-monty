# difffuzz divergence, seed 14705
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u177c'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᝼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c' | mm: EXC ValueError invalid literal for int() with base #: '᝼'
import math
try:
    try:
        print([((pow(int('  pad  ', 2), 12, 2 ** 61 - 1) ** (100 % 7)) + i6) for i6 in range(-2, 6)][math.gcd(((1e-05 ** 3) ** -1))])
    finally:
        print("finally", int(chr(6012)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

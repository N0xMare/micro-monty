# difffuzz divergence, seed 7114
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u0ff7'
#   mm:      EXC ValueError invalid literal for int() with base 0: '࿷'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ff#' | mm: EXC ValueError invalid literal for int() with base #: '࿷'
import math
try:
    print(divmod(pow(int(chr(4087), 0), 29, 2 ** 61 - 1), (sum(list(range(-3, 0, -1))) >> ((math.gcd(abs(v7) % 30) >> (int(v1) % 9)) % 9))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

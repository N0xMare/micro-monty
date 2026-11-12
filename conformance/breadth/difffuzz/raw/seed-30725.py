# difffuzz divergence, seed 30725
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u1f5a'
#   mm:      EXC ValueError invalid literal for int() with base 2: '὚'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#a' | mm: EXC ValueError invalid literal for int() with base #: '὚'
import math
try:
    n1 = 2
    while n1 > 0:
        n1 -= 1
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v3 = int(0.1)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    x = y = sorted([round((0.1 ** 3)), (pow(v3, 17, 7) ** ((n1 >> (v3 % 9)) % 7)), int(chr(8026), 2), divmod(abs(n1).bit_length(), int('3.5', 16))[1]])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 1231
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u05ff'
#   mm:      EXC ValueError invalid literal for int() with base 0: '׿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ff' | mm: EXC ValueError invalid literal for int() with base #: '׿'
import math
try:
    try:
        raise KeyError((42 >> (divmod(int(repr(1.5)), ((5 ** (-3 % 7)) - min([-3])))[0] % 9)))
    finally:
        print("finally", (math.factorial(abs(int(chr(1535), 0)) % 30) ** (max([(5 << (100 % 70))]) % 7)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

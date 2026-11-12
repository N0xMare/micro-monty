# difffuzz divergence, seed 40276
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0cdf'
#   mm:      EXC ValueError invalid literal for int() with base 10: '೟'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cdf' | mm: EXC ValueError invalid literal for int() with base #: '೟'
import math
try:
    try:
        print(int(chr(3295).ljust(1, chr(45))).nope)
    except LookupError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 6033
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u1ae8'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᫨'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ae#' | mm: EXC ValueError invalid literal for int() with base #: '᫨'
import math
try:
    try:
        print(list({str(q): q for q in list(range(0, 3, 3))}.values())[round(math.inf)])
    finally:
        print("finally", int(chr(6888)[:], 0))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

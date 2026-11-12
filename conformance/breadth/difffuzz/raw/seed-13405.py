# difffuzz divergence, seed 13405
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2e68'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⹨'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '⹨'
import math
try:
    for i15 in [(int(chr(11880)) + i16) for i16 in range(-1, 9)]:
        if i15 == 2:
            break
            def gen20(n):
                        yield k * 3
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

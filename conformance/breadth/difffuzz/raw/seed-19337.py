# difffuzz divergence, seed 19337
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: "-2-'mississippi'-1000\u0a7d"
#   mm:      EXC ValueError invalid literal for int() with base 10: "-2-'mississippi'-1000੽"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "-#-'mississippi'-#\u#a#d" | mm: EXC ValueError invalid literal for int() with base #: "-#-'mississippi'-#੽"
import math
try:
    for i30 in range(-2, 8):
        if i30 == 2:
            continue
        if (w := int(('{}-{!r}-{:>4}'.format(i30, 'mississippi', 1000).removeprefix('3.5'.rjust(6, chr(45))) + chr(2685)))) > 44:
            print("walrus", w)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

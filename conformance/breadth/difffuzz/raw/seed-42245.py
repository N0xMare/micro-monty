# difffuzz divergence, seed 42245
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: "0-'\\u193d'-   0"
#   mm:      EXC ValueError invalid literal for int() with base 16: "0-'᤽'-   0"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "#-'\\u#d'-   #" | mm: EXC ValueError invalid literal for int() with base #: "#-'᤽'-   #"
import math
try:
    n8 = 2
    while n8 > 0:
        n8 -= 1
        v9 = chr(2876).partition('s')[1].replace(str(42).join([]), chr(6461))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v13 = ((int('123', 10) + int('{}-{!r}-{:>4}'.format(n8, v9, n8), 16)) >> (int(repr({str(q): q for q in lst0})) % 9))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 21846
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: "2009170771914846595-'\\u0ef7'-   7"
#   mm:      EXC ValueError invalid literal for int() with base 10: "2009170771914846595-'໷'-   7"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "#-'\\u#ef#'-   #" | mm: EXC ValueError invalid literal for int() with base #: "#-'໷'-   #"
import math
try:
    v8 = int('{}-{!r}-{:>4}'.format(pow((5 << (-3 % 70)), 31, 2 ** 61 - 1), chr(3831), round(7.125)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

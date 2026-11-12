# difffuzz divergence, seed 38916
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2fdc'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⿜'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fdc' | mm: EXC ValueError invalid literal for int() with base #: '⿜'
import math
try:
    try:
        print(sorted(list(range(4, -1, 1)))[int(chr(12252))])
    except ArithmeticError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

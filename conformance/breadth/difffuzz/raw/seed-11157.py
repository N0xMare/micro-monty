# difffuzz divergence, seed 11157
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0c5c'
#   mm:      EXC ValueError invalid literal for int() with base 16: '౜'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c#c' | mm: EXC ValueError invalid literal for int() with base #: '౜'
import math
try:
    try:
        print(tuple(list(range(-1, 7, 1)))[-5:][int(chr(3164), 16)])
    except ArithmeticError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

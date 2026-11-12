# difffuzz divergence, seed 37606
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u0cfc'
#   mm:      EXC ValueError invalid literal for int() with base 0: '೼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cfc' | mm: EXC ValueError invalid literal for int() with base #: '೼'
import math
try:
    try:
        print(divmod(int(chr(3324), 0), i10)[0].nope)
    except ArithmeticError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

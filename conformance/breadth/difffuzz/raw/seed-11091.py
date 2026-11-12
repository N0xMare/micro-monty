# difffuzz divergence, seed 11091
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u05ec'
#   mm:      EXC ValueError invalid literal for int() with base 10: '׬'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ec' | mm: EXC ValueError invalid literal for int() with base #: '׬'
import math
try:
    def f8(p0, p1):
        v9 = (({p1: n6, '\U0001f600 ok': 1}['k1'] if p1.expandtabs().endswith(chr(4730)) else int(p0[::2])) < sum((lst0 * 2)[6:-1]) <= int(1e-05))
    print('f8(chr(1516), repr(7.125).zfill(6).title()[-6:1])', f8(chr(1516), repr(7.125).zfill(6).title()[-6:1]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

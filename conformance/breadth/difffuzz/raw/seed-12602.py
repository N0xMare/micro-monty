# difffuzz divergence, seed 12602
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0cb4'
#   mm:      EXC ValueError invalid literal for int() with base 16: '಴'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cb#' | mm: EXC ValueError invalid literal for int() with base #: '಴'
import math
try:
    n1 = 5
    while n1 > 0:
        class C2:
            def __init__(self, v, w=0):
                self.w = w
        objs = [C2(int(chr(3252), 16), -3), C2(dict(zip(('a,b,,c' + 'hello').split(), [(+n1 + i3) for i3 in sorted([-1, 42, 1, 3], key=abs) if i3 % 4]))['k2'], -2), C2(-(10 ** 25), 2), C2.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

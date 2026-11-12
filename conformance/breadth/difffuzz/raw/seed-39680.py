# difffuzz divergence, seed 39680
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0dd5------'
#   mm:      EXC ValueError invalid literal for int() with base 16: '෕------'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#dd#------' | mm: EXC ValueError invalid literal for int() with base #: '෕------'
import math
try:
    def f7(p0):
        try:
            print([1, 2].remove(int(1e10)))
        finally:
            print("finally", divmod([][int(p0.ljust(7, chr(45)), 16)], int(str([])[:1]))[1])
    print('f7(chr(3541))', f7(chr(3541)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

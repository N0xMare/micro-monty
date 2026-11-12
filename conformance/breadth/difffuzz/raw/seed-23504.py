# difffuzz divergence, seed 23504
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u139c'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᎜'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c' | mm: EXC ValueError invalid literal for int() with base #: '᎜'
import math
try:
    class C18:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C18([ord(c) for c in ('hello'.partition('s')[2] + '-42'.rjust(8, chr(45)))][int(chr(5020), 0)], -1), C18(-3, -1), C18(int('  pad  '.strip()[:], 10), -3), C18.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 10391
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u173f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᜿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f' | mm: EXC ValueError invalid literal for int() with base #: '᜿'
import math
try:
    class C12:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C12(int(chr(5951)), -3), C12(math.ceil(abs(len(())) % 30), 2), C12((int(chr(5623).zfill(3)) >> (round(abs((7.125 ** 0.5))) % 9)), 3), C12.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

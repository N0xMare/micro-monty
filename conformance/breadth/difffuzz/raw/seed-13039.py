# difffuzz divergence, seed 13039
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1af7'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᫷'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#af#' | mm: EXC ValueError invalid literal for int() with base #: '᫷'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C1(int(chr(6903)), -1), C1((len('-42') >> (int(math.inf) % 9)), 0), C1(-int(str((-0.0 ** 2)), 16), 3), C1.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

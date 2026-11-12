# difffuzz divergence, seed 18730
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u0df9'
#   mm:      EXC ValueError invalid literal for int() with base 0: '෹'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#df#' | mm: EXC ValueError invalid literal for int() with base #: '෹'
import math
try:
    class C6:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C6({self.v!r}, {self.w})'
    objs = [C6(sum([i + v for i, v in enumerate(list(filter(lambda q: q > 0, [])), 2)]), -3), C6((int(chr(3577), 0) ** (((1 << 100) - 1 ** (int(repr(v2)) % 7)) % 7)), 2), C6(42, -2), C6.make(1)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

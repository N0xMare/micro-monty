# difffuzz divergence, seed 6177
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u09e5'
#   mm:      EXC ValueError invalid literal for int() with base 0: '৥'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '৥'
import math
try:
    class C24:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C24((int(chr(2533), 0) ** (divmod(f2('tab\there'.removesuffix('mississippi')), ({'k0': 3} | {})['k1'])[0] % 7)), -2), C24(int(repr(-(-2.25 / 2.5)), 36), 3), C24(len({5, 2, 3}), -1), C24.make(1)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

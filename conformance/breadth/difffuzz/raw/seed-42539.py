# difffuzz divergence, seed 42539
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u17df'
#   mm:      EXC ValueError invalid literal for int() with base 36: '៟'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#df' | mm: EXC ValueError invalid literal for int() with base #: '៟'
import math
try:
    class C4:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C4(((int(chr(6111), 36) >> (~round(1e300) % 9)) + sorted([ord(c) for c in 'mississippi'], key=str)[(len({'k0': 1}) % (1 >> (10 % 9)))]), -1), C4(int(repr(['hello'])), -1), C4(min([ord(c) for c in '{}-{!r}-{:>4}'.format(round(3.0), 'x\ny', round(1e-05))]), 2), C4.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 24825
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1cfe'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᳾'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cfe' | mm: EXC ValueError invalid literal for int() with base #: '᳾'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.w = w
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C7(C1):
        def __init__(self, v, w=0):
            self.w = w
    objs = [C7(divmod(int(chr(7422)), (math.floor(abs(int(-2.25)) % 30) - int('AbC'.replace('-42', 'mississippi'))))[0], 1), C7(len([len(w) for w in list('a,b,,c'.join(['  pad  ', 'caf\u00e9']))]), -4), C7((min([10, sum([7, -17, -17, -3]), ~n5]) << ((len(chr(3421)) - int(math.atan(1e-05))) % 70)), 3), C7.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

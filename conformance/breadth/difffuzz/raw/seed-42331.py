# difffuzz divergence, seed 42331
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '0000\u20cb'
#   mm:      EXC ValueError invalid literal for int() with base 10: '0000⃋'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#cb' | mm: EXC ValueError invalid literal for int() with base #: '#⃋'
import math
try:
    class C2:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C2(int(chr(8395).zfill(5)), 2), C2(~pow((max([10, -17, 7, 3]) - math.factorial(-0.0)), 10, 1000), 0), C2(round(1e-05), -3), C2.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

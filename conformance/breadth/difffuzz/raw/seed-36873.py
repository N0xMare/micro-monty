# difffuzz divergence, seed 36873
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u17ee---------'
#   mm:      EXC ValueError invalid literal for int() with base 2: '៮---------'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ee---------' | mm: EXC ValueError invalid literal for int() with base #: '៮---------'
import math
try:
    def f4(p0):
        def rec5(n):
            if n <= 1:
                return n
        return int(chr(6126).ljust(10, chr(45)), 2)
    print('f4(int(round((divmod(42, 1000)[1] / f4(10)), 3)))', f4(int(round((divmod(42, 1000)[1] / f4(10)), 3))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

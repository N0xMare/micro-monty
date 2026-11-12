# difffuzz divergence, seed 4833
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: f5() '\U171C'
#   mm:      f5() '᜜'
# signature: CPython: f#() '\U#C' | mm: f#() '᜜'
import math
try:
    def f5():
        def rec6(n):
            if n <= 1:
                return n
        return repr(chr(5916)).swapcase()
    print('f5()', f5())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

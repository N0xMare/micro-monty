# difffuzz divergence, seed 360
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u245f'
#   mm:      EXC ValueError invalid literal for int() with base 36: '⑟'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f' | mm: EXC ValueError invalid literal for int() with base #: '⑟'
import math
try:
    n28 = 1
    while n28 > 0:
        class C29:
            def __init__(self, v, w=0):
                self.w = w
            def __repr__(self):
                return f'C29({self.v!r}, {self.w})'
        print(a.nope)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C38(C29):
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C38({self.v!r}, {self.w})'
    objs = [C38(-(2 ** 70), -4), C38(pow(int(chr(9311), 36), 28, 1000), 4), C38(divmod(str(isinstance(n28, object)).index(repr(v30.splitlines())), divmod(42, (v30 * (n33 % 4)).count(repr(7.125)))[1])[0], -1), C38.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

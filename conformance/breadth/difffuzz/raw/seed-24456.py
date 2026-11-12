# difffuzz divergence, seed 24456
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v43 {'\u0b9d': -17}
#   mm:      v43 {'஝': -17}
# signature: CPython: v# {'\u#b#d': -#} | mm: v# {'஝': -#}
import math
try:
        v43 = dict(zip(list(chr(2973)), sorted(({-3, 10, 42, 2, 42} ^ set([-17])))))
        print('v43', v43)
        class Ctx44:
            def __init__(self, name):
                self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

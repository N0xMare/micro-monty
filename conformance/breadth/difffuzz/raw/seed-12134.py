# difffuzz divergence, seed 12134
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC KeyError '\u09ff'
#   mm:      EXC KeyError '৿'
# signature: CPython: EXC KeyError '\u#ff' | mm: EXC KeyError '৿'
import math
try:
    n10 = 5
    while n10 > 0:
        v11 = {'k0': 2, 'k1': 100}
        class C12:
            def __init__(self, v, w=0):
                self.w = w
        print({o: o.w for o in objs}[objs[0]], getattr(a, "missing", "dflt"))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    del v11[chr(2559)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

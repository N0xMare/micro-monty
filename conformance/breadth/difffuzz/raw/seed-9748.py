# difffuzz divergence, seed 9748
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError ᯺ ('\u1bfa',)
#   mm:      caught ValueError ᯺ ('᯺',)
# signature: CPython: caught ValueError ᯺ ('\u#bfa',) | mm: caught ValueError ᯺ ('᯺',)
import math
try:
    n1 = 5
    while n1 > 0:
        def risky():
            try:
                raise ValueError(chr(7162).upper())
            except Exception as e:
                print("caught", type(e).__name__, e, e.args)
        print(risky())
        class C2:
            def __init__(self, v, w=0):
                self.w = w
        print({o: o.w for o in objs}[objs[0]], getattr(a, "missing", "dflt"))
        def make4(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

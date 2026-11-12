# difffuzz divergence, seed 2811
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: 3
#   mm:      EXC MemoryError 
# signature: CPython: # | mm: EXC MemoryError 
import math
try:
    if (((2 ** 64 ** (pow(255, 35, 7) % 7)) in {3, 255, 5, 42}) or True):
        if False:
            print(next(g, None), list(g))
    else:
        while n4 > 0:
            class Err5(ValueError):
                    self.code = code
            try:
                try:
                    raise Err5(str(lst0).partition('s')[1].join('tab\there'[n4].split(',')).join(list({'k0': 255, 'k1': 7, 'k2': 100})), 6)
                except Err5 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
            def risky():
                try:
                    print(len((tuple(lst0[:]) + (n4, '', )[:3])) ** -2)
                except (TypeError, ValueError) as e:
                    print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def rec7(n):
        if n <= 1:
            return n
        return rec7(n - 1) + rec7(n - 2)
    print(rec7(4))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

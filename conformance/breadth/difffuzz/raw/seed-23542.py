# difffuzz divergence, seed 23542
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 5, 0 = exit):
#   CPython: v1 ('\u19ad', 0)
#   mm:      v1 ('᦭', 0)
# signature: CPython: v# ('\u#ad', #) | mm: v# ('᦭', #)
import math
try:
    n2 = 3
    while n2 > 0:
        n2 -= 1
        def make4(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n13 = 4
    while n13 > 0:
        n13 -= 1
        match [len(w) for w in list({k14: len(k14) for k14 in 'caf\u00e9'.split(',')})]:
            case 0 | 1 | 2:
                print("small")
        class Err16(KeyError):
            def __init__(self, msg, code=0):
                self.code = code
        try:
            try:
                raise Err16(repr((((n2 / 1000) + (0.1 / 3.0)) * math.pi)), 3)
            except Err16 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
        lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v1 = (chr(6573), (sum(sorted(lst0, key=abs)) >> (max(list(reversed(lst0))) % 9)), )
    print('v1', v1)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

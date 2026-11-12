# difffuzz divergence, seed 33507
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v7 ["['\\u0e6e']"]
#   mm:      v7 ["['๮']"]
# signature: CPython: v# ["['\\u#e#e']"] | mm: v# ["['๮']"]
import math
try:
    n2 = 3
    while n2 > 0:
        n2 -= 1
        n3 = 6
        while n3 > 0:
            data = [str(q) for q in list(map(lambda q: q * 1 + 1, [sum([5, -17, -17])]))]
            n3 = [(round(-2.25) + i5) for i5 in range(2)][len({s6 % 3 for s6 in []})]
            if bool(3.0):
                v7 = repr(chr(3694).splitlines()).splitlines()
                print('v7', v7)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

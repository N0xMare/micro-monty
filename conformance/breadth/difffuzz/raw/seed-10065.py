# difffuzz divergence, seed 10065
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: "['\\u20c5']"
#   mm:      EXC ValueError invalid literal for int() with base 0: "['⃅']"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "['\\u#c#']" | mm: EXC ValueError invalid literal for int() with base #: "['⃅']"
import math
try:
    n19 = 4
    while n19 > 0:
        match int(repr(chr(8389).split(' ')), 0):
            case {}:
                print("any mapping")
        def f20(p0, p1, *rest, kw=5):
            if (int(float(p1.count(p1))) < sum(sorted(list(filter(lambda q: q > 5, p0)), key=str)) <= (math.trunc(abs(f13(p1, p1, p1)) % 30) ** (([][n19] ** (p0[5] % 7)) % 7))):
                print(p0)
                def __init__(self, v, w=0):
                    self.w = w
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

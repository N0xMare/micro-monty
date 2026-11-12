# difffuzz divergence, seed 12581
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ('\u2067',)ßcaught ValueError max() iterable argument is empty ('max() iterable argument is empty',)
#   mm:      ('⁧',)ßcaught ValueError max() iterable argument is empty ('max() iterable argument is empty',)
# signature: CPython: ('\u#',)ßcaught ValueError max() iterable argument is empty ('max() iterable argument is empty',) | mm: ('⁧',)ßcaught ValueError max() iterable argument is empty ('max() iterable argument is empty',)
import math
try:
    def f11(p0, p1, p2):
        def make12(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print((chr(8295)[-2:], ), end='\u00df')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i14 in range(7):
        if i14 == 4:
            continue
        def risky():
            try:
                print(f11(list(filter(lambda q: q > 5, sorted({-1, -17, 1, 255}))), list(filter(lambda q: q > 4, [i + v for i, v in enumerate([(i14 + i15) for i15 in [1, -3, 255, 0, 2] if i15 % 4], 2)])), f'{None!s} {max([]):#x}').index((chr(9257).swapcase() * (-3 % 4))))
            except Exception as e:
                print("caught", type(e).__name__, e, e.args)
        print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

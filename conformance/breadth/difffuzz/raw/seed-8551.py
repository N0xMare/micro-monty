# difffuzz divergence, seed 8551
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0a0c'
#   mm:      EXC ValueError invalid literal for int() with base 10: '਌'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#c' | mm: EXC ValueError invalid literal for int() with base #: '਌'
import math
try:
    for i13 in range(2):
        n14 = 3
        while n14 > 0:
            try:
                print(chr(i13))
            except (TypeError, ValueError) as e:
                print("caught", type(e).__name__, e, e.args)
            def risky():
                try:
                    print(int(chr(2572)))
                except KeyError as e:
                    print("caught", type(e).__name__, e, e.args)
            print(risky())
        def rec20(n):
            if n <= 1:
                return n
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

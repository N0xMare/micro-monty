# difffuzz divergence, seed 17617
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u0a50'
#   mm:      EXC ValueError invalid literal for int() with base 36: '੐'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#' | mm: EXC ValueError invalid literal for int() with base #: '੐'
import math
try:
    if (sum([a * b for a, b in zip([-1, -3], sorted([3], reverse=True))]) < int(chr(2640), 36) <= math.factorial(abs((sum(lst0) if (False and False) else math.factorial(1e300))) % 30)):
        try:
            print(tuple(sorted({('a' + 'hello').title(): (max([0, 0, 42, 1]) - -5)}.items()))[v2['k2']])
        except ArithmeticError as e:
            print("caught", type(e).__name__, e, e.args)
            try:
                raise ValueError('')
            except Exception as e:
                print("caught", type(e).__name__, e, e.args)
        for i32 in range(1, 7):
            if i32 == 1:
                break
        def make34(start):
            total = start
            def step(by=1):
                return total
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

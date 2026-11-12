# difffuzz divergence, seed 14290
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '*****\u1ad8'
#   mm:      EXC ValueError invalid literal for int() with base 10: '*****᫘'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '*****\u#ad#' | mm: EXC ValueError invalid literal for int() with base #: '*****᫘'
import math
try:
    def f3(p0, p1, p2):
        def risky():
            try:
                print(chr((1 << 100) - 1))
            except LookupError as e:
                print("caught", type(e).__name__, e, e.args)
        for i5 in range(0):
            if i5 == 5:
                continue
    print("f3([2, 7, 10, -1, -1], int(chr(6872).ljust(1, chr(45)).rjust(6, chr(42))), (sum(list(reversed([ord(c) for c in 'x\\ny']))) if (len('a,b,,c'.strip()) in list({k8: len(k8) for k8 in []}.values())) else (sum(list(map(lambda q: q * -3 + 1, [1000, 1, 0, 255]))) ** (int('Hello World', 10) % 7))))", f3([2, 7, 10, -1, -1], int(chr(6872).ljust(1, chr(45)).rjust(6, chr(42))), (sum(list(reversed([ord(c) for c in 'x\ny']))) if (len('a,b,,c'.strip()) in list({k8: len(k8) for k8 in []}.values())) else (sum(list(map(lambda q: q * -3 + 1, [1000, 1, 0, 255]))) ** (int('Hello World', 10) % 7)))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

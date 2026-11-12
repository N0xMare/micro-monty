# difffuzz divergence, seed 23216
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0be4'
#   mm:      EXC ValueError invalid literal for int() with base 16: '௤'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#be#' | mm: EXC ValueError invalid literal for int() with base #: '௤'
import math
try:
    def f18(p0, p1, *rest, kw=3):
        match (1, "x"):
            case str(s) if len(s) > 3:
                print("long", s[:3])
    print('f18([(int(chr(3044), 16) + i19) for i19 in range(3) if i19 % 4], min(sorted([5, 100, -1, 42], key=lambda q: (q % 3, -q))[5::1][0:]))', f18([(int(chr(3044), 16) + i19) for i19 in range(3) if i19 % 4], min(sorted([5, 100, -1, 42], key=lambda q: (q % 3, -q))[5::1][0:])))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

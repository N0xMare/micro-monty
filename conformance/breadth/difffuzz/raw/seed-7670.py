# difffuzz divergence, seed 7670
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0cf9'
#   mm:      EXC ValueError invalid literal for int() with base 10: '೹'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cf#' | mm: EXC ValueError invalid literal for int() with base #: '೹'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C1(int(chr(3321).zfill(0).expandtabs()), -1), C1(math.gcd((len({100, 255}) / list({'k0': 7}.values())[int('a,b,,c')])), -2), C1(min(([3, 3] * 3)), -2), C1.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

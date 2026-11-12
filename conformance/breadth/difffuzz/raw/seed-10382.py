# difffuzz divergence, seed 10382
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1f46'
#   mm:      EXC ValueError invalid literal for int() with base 10: '὆'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#' | mm: EXC ValueError invalid literal for int() with base #: '὆'
import math
try:
    class C3:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C3((int(chr(8006).replace(str(True), '{}-{!r}-{:>4}'.format(-3, '  pad  ', 7)), 10) << (round(1e-05) % 70)), 1), C3(+(('\u00df' * (-1 % 4)).count('a,b,,c'.zfill(3)) >> (list({'k0': 2, 'k1': 42, 'k2': 3}.values())[pow(10, 9, -5)] % 9)), 0), C3((max(list(map(lambda q: q * 2 + 1, sorted([10, 0, -17, 255, 1], key=lambda q: (q % 3, -q))))) ** (pow(sum([][:]), 17, 7) % 7)), -1), C3.make(1)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

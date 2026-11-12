# difffuzz divergence, seed 21809
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: "'\\u070e'"
#   mm:      EXC ValueError invalid literal for int() with base 10: "'܎'"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "'\\u#e'" | mm: EXC ValueError invalid literal for int() with base #: "'܎'"
import math
try:
    if all(sorted(list(range(5, 12, 3)), key=abs)):
        class C12:
            def __init__(self, v, w=0):
                self.w = w
        objs = [C12(int(repr(chr(1806))), 3), C12(repr(7.125).find(('a'.zfill(4).replace(repr(['AbC', 'a,b,,c', 'a,b,,c']), '\u00df'.partition('s')[2]) + 'caf\u00e9'[:6])), -2), C12(math.gcd(abs(sorted(sorted({-3, 10, 2, 2}), reverse=True)[[1, 100, 100, 255, -1][(1000 ** (-17 % 7))]]) % 30), -3), C12.make(3)]
        class Ctx13:
            def __init__(self, name):
                self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

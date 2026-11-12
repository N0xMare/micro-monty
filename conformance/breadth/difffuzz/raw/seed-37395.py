# difffuzz divergence, seed 37395
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: "2-'1.500000e+00'-   3\u2e79"
#   mm:      EXC ValueError invalid literal for int() with base 10: "2-'1.500000e+00'-   3⹹"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "#-'#.#e+#'-   #\u#e#" | mm: EXC ValueError invalid literal for int() with base #: "#-'#.#e+#'-   #⹹"
import math
try:
    n7 = 4
    while n7 > 0:
        def f8(p0):
            return int(('{}-{!r}-{:>4}'.format(len({'k0': 7, 'k1': 255}), '%e' % (1.5,), math.gcd(abs(3) % 30)) + chr(11897)))
        print('f8([i + v for i, v in enumerate((list(reversed(sorted({2, 1000, 42}))) * 2), 0)])', f8([i + v for i, v in enumerate((list(reversed(sorted({2, 1000, 42}))) * 2), 0)]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 9306
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0df6True'
#   mm:      EXC ValueError invalid literal for int() with base 16: '෶True'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#df#True' | mm: EXC ValueError invalid literal for int() with base #: '෶True'
import math
try:
    def make20(start):
        total = start
        def step(by=1):
            return total
    s = make20(int((chr(3574) + str(True).replace(f'{[3, 2, 1, 10, -1]!r}|{None!s} ', ('AbC' * (1000 % 4)))), 16))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 42878
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1f4f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '὏'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#f' | mm: EXC ValueError invalid literal for int() with base #: '὏'
import math
try:
    n8 = 3
    while n8 > 0:
        match ({'k0': 7, 'k1': 42} | dict(sorted({'k0': 1000, 'k1': 0, 'k2': 5, 'k3': 100}.items(), key=lambda kv: kv[1]))):
            case str(s) if len(s) > 3:
                print("long", s[:3])
        class C9:
            def __init__(self, v, w=0):
                self.w = w
            def __repr__(self):
                return f'C9({self.v!r}, {self.w})'
        objs = [C9(math.factorial(abs(n8) % 30), 2), C9(math.factorial(abs(n8) % 30), -1), C9(((int(chr(8015)) >> (({'k0': 10} | {'k0': 42})['k2'] % 9)) % round((0.0 + (n8 / 0)))), 2), C9.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

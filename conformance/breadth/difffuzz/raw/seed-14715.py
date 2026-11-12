# difffuzz divergence, seed 14715
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1a8f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᪏'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#f' | mm: EXC ValueError invalid literal for int() with base #: '᪏'
import math
try:
    class C17:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C17(int((chr(6799) * (math.floor(abs(abs(1).bit_length()) % 30) % 4))), -1), C17(+(42 ** ((len({}) if ([100] == [7, 1, 5]) else (1000 >> (0 % 9))) % 7)), 0), C17(list(({'k0': 1, 'k1': 1, 'k2': 3, 'k3': 100} | ({'k0': -3} | {'k0': 10})).values())[len(())], -2), C17.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

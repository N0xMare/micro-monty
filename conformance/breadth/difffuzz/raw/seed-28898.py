# difffuzz divergence, seed 28898
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '00000\u0c4f'
#   mm:      EXC ValueError invalid literal for int() with base 16: '00000౏'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#c#f' | mm: EXC ValueError invalid literal for int() with base #: '#౏'
import math
try:
    class C17:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C17({self.v!r}, {self.w})'
    objs = [C17(len(list(repr(2.5).swapcase())), 4), C17((int(chr(3151).zfill(6), 16) >> (((min([]) if 'hello'.endswith('-42') else (-17 ** (255 % 7))) if (min(lst0) in [i + v for i, v in enumerate(lst0, 0)]) else (int('\u00df') << (abs(255).bit_length() % 70))) % 9)), -2), C17((({'k0': -3}['k1'] >> (round(1e300) % 9)) >> (divmod(((i12 * 1) ** (divmod(i12, i12)[0] % 7)), 10 ** 30 + 7)[0] % 9)), -3), C17.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

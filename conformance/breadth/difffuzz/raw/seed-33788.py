# difffuzz divergence, seed 33788
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0eda'
#   mm:      EXC ValueError invalid literal for int() with base 10: '໚'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#eda' | mm: EXC ValueError invalid literal for int() with base #: '໚'
import math
try:
        v16 = chr(3802)
        class Ctx17:
            def __init__(self, name):
                self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C27:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C27((list(({'k0': 7, 'k1': 255, 'k2': 100} | {'k0': 5, 'k1': 100, 'k2': -1}).values())[divmod(int(v16), (1 ** (i19 % 7)))[0]] ** ((dict(zip([], lst0))['k1'] >> (int(f'{lst0!r} and {v16!a} {lst0!r} and ') % 9)) % 7)), 0), C27(divmod((10 ** 30 + 7 if ((7.125 ** 3) >= i15) else abs(math.floor(0.0)).bit_length()), {k28: len(k28) for k28 in v16.splitlines()}['zz'])[0], -4), C27((3 ** 50 >> (int('x\ny'.replace(v16, 'tab\there').join(''.split(','))) % 9)), 4), C27.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

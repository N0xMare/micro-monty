# difffuzz divergence, seed 15953
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '00000000\u169e'
#   mm:      EXC ValueError invalid literal for int() with base 10: '00000000᚞'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#e' | mm: EXC ValueError invalid literal for int() with base #: '#᚞'
import math
try:
    class C13:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C13(int(chr(5790).zfill(9)), -2), C13('%%%d' % (sum([100]), f1((0 ** (2 % 7)), int('a', 36)),).index(v8.expandtabs().partition(',')[0][list(reversed(lst0))[divmod(-3, -1)[1]]]), 1), C13({'k0': 2, 'k1': 1, 'k2': 100, 'k3': 10}['k2'], -1), C13.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

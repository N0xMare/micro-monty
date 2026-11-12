# difffuzz divergence, seed 41409
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1a5f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᩟'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#f' | mm: EXC ValueError invalid literal for int() with base #: '᩟'
import math
try:
    v10 = (round(math.log2((42 / 1)), 3) * -math.tau)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n11 = 4
    while n11 > 0:
        def f12(p0, p1, p2):
            class C13:
                def __init__(self, v, w=0):
                    self.w = w
            objs = [C13(int(chr(6751)), -2), C13(-(10 ** 25), -1), C13(f2('AbC', '', -17).split(' ')[1]['-42'.split('s')[-1].rfind('-42'.split('s')[1])].index('%-6s|' % ('x\ny', 5,).swapcase().title()), -2), C13.make(4)]
        print("f12(int((abs(v10) ** 0.5)), (n11 << (sum([ord(c) for c in '3.5'[5:0]]) % 70)), n11)", f12(int((abs(v10) ** 0.5)), (n11 << (sum([ord(c) for c in '3.5'[5:0]]) % 70)), n11))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

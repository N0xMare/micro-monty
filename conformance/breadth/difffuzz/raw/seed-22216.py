# difffuzz divergence, seed 22216
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1c8f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᲏'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c#f' | mm: EXC ValueError invalid literal for int() with base #: '᲏'
import math
try:
    class C2:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C2(int(chr(7311)), 4), C2('%s|%r' % ('a,b,,c'.replace('-42', 'hello'),).split(' ')[-2].find('%05d' % (math.pi,)), -4), C2(divmod(pow(list(filter(lambda q: q > -1, [-3, 5]))[(2 if False else 10)], 8, 7), round((sum([-1, 10]) / 100)))[1], -1), C2.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 19466
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2e7f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⹿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#f' | mm: EXC ValueError invalid literal for int() with base #: '⹿'
import math
try:
    def f16():
        def make17(start):
            total = start
            def step(by=1):
                return total
        s = make17(int(chr(11903).split(None)[0]))
        if isinstance(list(filter(lambda q: q > 5, [len(w) for w in ['a', 'caf\u00e9']]))[min(sorted(lst0))], object):
            class Ctx19:
                def __init__(self, name):
                    self.name = name
    print('f16()', f16())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

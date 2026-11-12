# difffuzz divergence, seed 15564
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u2fee'
#   mm:      EXC ValueError invalid literal for int() with base 2: '⿮'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fee' | mm: EXC ValueError invalid literal for int() with base #: '⿮'
import math
try:
    def f1(p0, p1, p2):
        def gen2(n):
            for k in range(n):
                    yield k * 0
        it = gen2(7)
        return list(filter(lambda q: q > 1, sorted([ord(c) for c in '123'])))['%d' % (int(p1, 2), p1.capitalize(),).index(chr(3159))]
    print("f1([len(w) for w in []], chr(12270), [ord(c) for c in '%5.2f' % (float('3.5'),)][5::-2])", f1([len(w) for w in []], chr(12270), [ord(c) for c in '%5.2f' % (float('3.5'),)][5::-2]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

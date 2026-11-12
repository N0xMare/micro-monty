# difffuzz divergence, seed 29804
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0b79😀'
#   mm:      EXC ValueError could not convert string to float: '୹😀'
# signature: CPython: EXC ValueError could not convert string to float: '\u#b#😀' | mm: EXC ValueError could not convert string to float: '୹😀'
import math
try:
    if (str('a'.isalpha()).partition('s')[1] in f'{10:c} and '.replace((repr('  pad  ') + '{}-{!r}-{:>4}'.format(5, 'caf\u00e9', 1000)), repr({'k0': 7, 'k1': 1}).expandtabs())):
        n1 = 3
        while n1 > 0:
            def make2(start):
                total = start
                def step(by=1):
                    return total
            s = make2(round(float((chr(2937) + '\U0001f600 ok'.split()[-2]))))
            def __init__(self, msg, code=0):
                self.code = code
        try:
            try:
                raise Err4(chr(338), 7)
            except Err4 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

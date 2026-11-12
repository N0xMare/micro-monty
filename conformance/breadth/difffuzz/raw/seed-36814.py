# difffuzz divergence, seed 36814
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '----------\u2fe0'
#   mm:      EXC ValueError invalid literal for int() with base 10: '----------⿠'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '----------\u#fe#' | mm: EXC ValueError invalid literal for int() with base #: '----------⿠'
import math
try:
    n10 = 2
    while n10 > 0:
        def rec11(n):
            if n <= 1:
                return n
        try:
            print(int(chr(12256)[-1:].rjust(11, chr(45))))
        except ZeroDivisionError as e:
            print("caught", type(e).__name__, e, e.args)
        if (int(('\u00df'.zfill(8) + chr(10453))) in set([(max([1, 0, 0]) + i13) for i13 in range(-1, 5)])):
            try:
                print(repr(['  pad  ']) + sum([i + v for i, v in enumerate([divmod(10, i4)[0], [-1, 1][i6]], 0)]))
            finally:
                print("finally", int((int('x\ny'.removeprefix('x\ny')) / len({k16: len(k16) for k16 in ['x\ny']}))))
            match {str(q): q for q in sorted([len(w) for w in 'mississippi'.split(' ')], key=str)}:
                case 0 | 1 | 2:
                    print("small")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

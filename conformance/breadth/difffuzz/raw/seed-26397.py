# difffuzz divergence, seed 26397
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0b12'
#   mm:      EXC ValueError invalid literal for int() with base 16: '଒'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '଒'
import math
try:
    def f7(p0, p1, p2):
        return repr([])
    print("f7(list({str(q): q for q in list(filter(lambda q: q > 0, [5, 255, 1, 1][:]))}.values()), 'hello'[:], int(chr(2834), 16))", f7(list({str(q): q for q in list(filter(lambda q: q > 0, [5, 255, 1, 1][:]))}.values()), 'hello'[:], int(chr(2834), 16)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

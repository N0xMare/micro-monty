# difffuzz divergence, seed 2444
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0c49'
#   mm:      EXC ValueError invalid literal for int() with base 10: '౉'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c#' | mm: EXC ValueError invalid literal for int() with base #: '౉'
import math
try:
    if bool(math.inf):
        print(list(map(lambda q: q * 0 + 1, sorted(set([10, 2]))))[int(chr(3145))], sep='x\ny')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

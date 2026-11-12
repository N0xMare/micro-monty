# difffuzz divergence, seed 5685
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0df1'
#   mm:      EXC ValueError invalid literal for int() with base 10: '෱'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#df#' | mm: EXC ValueError invalid literal for int() with base #: '෱'
import math
try:
    def f6(p0, p1, p2):
        return i1
    print("f6(int(chr(3569)), sorted({7, 100, 2, 10}), (v5 + '  pad  '.replace(v5, v5).title().expandtabs()))", f6(int(chr(3569)), sorted({7, 100, 2, 10}), (v5 + '  pad  '.replace(v5, v5).title().expandtabs())))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

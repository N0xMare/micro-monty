# difffuzz divergence, seed 43932
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0df1'
#   mm:      EXC ValueError invalid literal for int() with base 10: '෱'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#df#' | mm: EXC ValueError invalid literal for int() with base #: '෱'
import math
try:
    def make45(start):
        total = start
        def step(by=1):
            return total
    s = make45(int(chr(3569)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

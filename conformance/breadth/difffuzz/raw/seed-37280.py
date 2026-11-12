# difffuzz divergence, seed 37280
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1af4'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᫴'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#af#' | mm: EXC ValueError invalid literal for int() with base #: '᫴'
import math
try:
    print(int(chr(6900)), sep='\U0001f600 ok')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 32591
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0dbc-----------'
#   mm:      EXC ValueError invalid literal for int() with base 10: '඼-----------'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#dbc-----------' | mm: EXC ValueError invalid literal for int() with base #: '඼-----------'
import math
try:
    match '%s|%r' % (math.pi,)[int(chr(3516).ljust(12, chr(45)), 10)]:
        case [] | ():
            print("empty")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

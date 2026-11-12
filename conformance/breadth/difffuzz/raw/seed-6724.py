# difffuzz divergence, seed 6724
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u1af5'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᫵'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#af#' | mm: EXC ValueError invalid literal for int() with base #: '᫵'
import math
try:
    try:
        print(int(chr(6901).removesuffix(chr(2111)), 0) ** -2)
    except KeyError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

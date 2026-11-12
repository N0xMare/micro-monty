# difffuzz divergence, seed 15183
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: "'-------a'\u0af5"
#   mm:      EXC ValueError invalid literal for int() with base 0: "'-------a'૵"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "'-------a'\u#af#" | mm: EXC ValueError invalid literal for int() with base #: "'-------a'૵"
import math
try:
    (a, b), c = (), int((repr('a'.rjust(8, chr(45))) + chr(2805)), 0)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

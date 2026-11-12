# difffuzz divergence, seed 37026
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '😀 ok\u19db'
#   mm:      EXC ValueError invalid literal for int() with base 10: '😀 ok᧛'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '😀 ok\u#db' | mm: EXC ValueError invalid literal for int() with base #: '😀 ok᧛'
import math
try:
    print(int(('\U0001f600 ok' + chr(6619))), sep='Hello World')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 39949
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '000\u09d3'
#   mm:      EXC ValueError invalid literal for int() with base 10: '000৓'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#d#' | mm: EXC ValueError invalid literal for int() with base #: '#৓'
import math
try:
    print(int((('%d' % (0.1,) * (len(['3.5', 'hello', '  pad  ']) % 4)) + chr(2515))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

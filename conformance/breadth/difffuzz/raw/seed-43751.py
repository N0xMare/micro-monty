# difffuzz divergence, seed 43751
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u16fb  pad  '
#   mm:      EXC ValueError invalid literal for int() with base 10: '᛻  pad  '
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fb  pad  ' | mm: EXC ValueError invalid literal for int() with base #: '᛻  pad  '
import math
try:
    print(int((chr(5883) + '  pad  '.lstrip('hello')).split(',')[-1]), end='3.5')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

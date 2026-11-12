# difffuzz divergence, seed 31782
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u191f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᤟'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f' | mm: EXC ValueError invalid literal for int() with base #: '᤟'
import math
try:
    v8 = ((int(chr(6431), 10) if '\U0001f600 ok'.lstrip('a').islower() else ((1 % 100) >> ((-17 << (255 % 70)) % 9))), chr(11813), )
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 31196
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '00000\u18ae'
#   mm:      EXC ValueError invalid literal for int() with base 2: '00000᢮'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#ae' | mm: EXC ValueError invalid literal for int() with base #: '#᢮'
import math
try:
    print(+int(chr(6318).zfill(6), 2), end='caf\u00e9')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

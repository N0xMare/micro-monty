# difffuzz divergence, seed 16816
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '0000\u18ab'
#   mm:      EXC ValueError invalid literal for int() with base 10: '0000᢫'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#ab' | mm: EXC ValueError invalid literal for int() with base #: '#᢫'
import math
try:
    if (w := int(chr(6315).split(' ')[0].zfill(5))) > 42:
        print("walrus", w)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

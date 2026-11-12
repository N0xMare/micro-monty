# difffuzz divergence, seed 10177
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '123\u175c'
#   mm:      EXC ValueError invalid literal for int() with base 10: '123᝜'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#c' | mm: EXC ValueError invalid literal for int() with base #: '#᝜'
import math
try:
    match abs(int(('123'[:] + chr(5980)))).bit_length():
        case list(l):
            print("list", len(l))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

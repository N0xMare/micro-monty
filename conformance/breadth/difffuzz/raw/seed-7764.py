# difffuzz divergence, seed 7764
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u0bbc'
#   mm:      EXC ValueError invalid literal for int() with base 36: '஼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#bbc' | mm: EXC ValueError invalid literal for int() with base #: '஼'
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def risky():
        try:
            print((((int(7.125) if (True or True) else (7 * 1000)) << (len({'caf\u00e9': 1000}) % 70)) >> (len(tuple(list(reversed(lst0)))) % 9)) // int(chr(3004), 36))
        finally:
            print("finally", -(10 ** 25))
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

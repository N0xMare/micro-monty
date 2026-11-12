# difffuzz divergence, seed 34797
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2e7b'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⹻'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#b' | mm: EXC ValueError invalid literal for int() with base #: '⹻'
import math
try:
    def risky():
        try:
            print('hello'.index('caf\u00e9'))
        finally:
            print("finally", int(chr(11899)))
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

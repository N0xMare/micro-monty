# difffuzz divergence, seed 25914
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0f6e'
#   mm:      EXC ValueError invalid literal for int() with base 10: '཮'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#e' | mm: EXC ValueError invalid literal for int() with base #: '཮'
import math
try:
    def f22(p0, p1):
        match chr(9610).upper():
            case str(s) if len(s) > 3:
                print("long", s[:3])
    print('f22(int(chr(3950)), chr(8272))', f22(int(chr(3950)), chr(8272)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

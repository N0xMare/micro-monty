# difffuzz divergence, seed 38657
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u1c4a'
#   mm:      EXC ValueError invalid literal for int() with base 2: '᱊'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c#a' | mm: EXC ValueError invalid literal for int() with base #: '᱊'
import math
try:
    for i9 in range(-1, 7):
        if i9 == 3:
            continue
        try:
            print(round(((i9 >> ((i9 if False else -3) % 9)) / ((i9 << (i9 % 70)) // (i9 >> (i9 % 9))))) // math.floor(abs(~i9) % 30))
        except KeyError as e:
            print("caught", type(e).__name__, e, e.args)
        finally:
            print("finally", int(chr(7242).strip().removeprefix(''.partition(',')[1].swapcase()), 2))
            def __init__(self, v, w=0):
                self.w = w
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

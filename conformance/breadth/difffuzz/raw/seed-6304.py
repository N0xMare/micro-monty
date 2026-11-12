# difffuzz divergence, seed 6304
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0a0e'
#   mm:      EXC ValueError invalid literal for int() with base 10: '਎'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#e' | mm: EXC ValueError invalid literal for int() with base #: '਎'
import math
try:
    def f18(p0):
        for i19 in range(3, 6):
            def rec20(n):
                if n <= 1:
                    return n
            try:
                try:
                    raise Err21(str(int(abs(round(-2.25, -2)))), 6)
                except Err21 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
    print("f18('a'[int(chr(2574))])", f18('a'[int(chr(2574))]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

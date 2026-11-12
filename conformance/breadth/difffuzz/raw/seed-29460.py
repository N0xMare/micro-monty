# difffuzz divergence, seed 29460
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0a3a'
#   mm:      EXC ValueError invalid literal for int() with base 10: '਺'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#a' | mm: EXC ValueError invalid literal for int() with base #: '਺'
import math
try:
    for i4 in range(4):
        if i4 == 2:
            continue
            def __init__(self, msg, code=0):
                self.code = code
        try:
            try:
                raise Err6(chr(10743), 8)
            except Err6 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(''[5:8].replace('%05d' % (divmod((0 >> (-3 % 9)), (7 ** (i4 % 7)))[0], int(chr(2618)),), '%05d' % (math.ceil(abs(v2) % 30),)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

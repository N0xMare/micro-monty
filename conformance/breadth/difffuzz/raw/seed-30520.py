# difffuzz divergence, seed 30520
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2439'
#   mm:      EXC ValueError invalid literal for int() with base 10: '␹'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '␹'
import math
try:
    n19 = 4
    while n19 > 0:
        class Err20(Exception):
            def __init__(self, msg, code=0):
                self.code = code
        try:
            try:
                raise Err20(f'{list(filter(lambda q: q > 0, [3, 255, 255, 42]))!r}|{n19:08b}|{int(chr(9273), 10):+08,} ', 0)
            except Err20 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
        match None:
            case int(n) if n > 10:
                print("big", n)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

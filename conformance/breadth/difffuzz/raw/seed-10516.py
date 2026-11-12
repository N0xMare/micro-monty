# difffuzz divergence, seed 10516
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u1c8d'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᲍'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c#d' | mm: EXC ValueError invalid literal for int() with base #: '᲍'
import math
try:
    class Err32(KeyError):
        def __init__(self, msg, code=0):
            self.code = code
    try:
        try:
            raise Err32('%%%d' % (len([100]), int(chr(7309), 0),).partition(' ')[2], 9)
        except Err32 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

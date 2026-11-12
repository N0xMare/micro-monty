# difffuzz divergence, seed 38437
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u09db'
#   mm:      EXC ValueError invalid literal for int() with base 10: '৛'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#db' | mm: EXC ValueError invalid literal for int() with base #: '৛'
import math
try:
        class Err2(ValueError):
            def __init__(self, msg, code=0):
                self.code = code
        try:
            try:
                raise Err2('{}-{!r}-{:>4}'.format(+{'k0': 3, 'k1': -3, 'k2': 3}['k1'], str(isinstance('123', list)), int(chr(2523), 10)).partition(' ')[2], 2)
            except Err2 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

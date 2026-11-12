# difffuzz divergence, seed 34437
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v20 ['c\u0dcc']
#   mm:      v20 ['c෌']
# signature: CPython: v# ['c\u#dcc'] | mm: v# ['c෌']
import math
try:
    n1 = 0
    while n1 > 0:
        class Ctx2:
            def __init__(self, name):
                self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v4 = f'{[3]!r} {2.5=}|'.strip().rjust(4, chr(45))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v20 = ('caf\u00e9'[n1].replace(v4.capitalize(), f'{n1:+}|') + chr(3532)).splitlines()
    print('v20', v20)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

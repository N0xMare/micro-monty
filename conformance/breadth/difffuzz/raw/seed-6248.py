# difffuzz divergence, seed 6248
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['*****\u1aeb*****']
#   mm:      ['*****᫫*****']
# signature: CPython: ['*****\u#aeb*****'] | mm: ['*****᫫*****']
import math
try:
        data = chr(6891).center(11, chr(42)).splitlines()
        print(sorted(data, key=lambda q: (str(q), q), reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

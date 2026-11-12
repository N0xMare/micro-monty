# difffuzz divergence, seed 37661
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: 0-'\u0b7b'-   20-'\u0b7b'-   20-'\u0b7b'-   2
#   mm:      0-'୻'-   20-'୻'-   20-'୻'-   2
# signature: CPython: #-'\u#b#b'-   #-'\u#b#b'-   #-'\u#b#b'-   # | mm: #-'୻'-   #-'୻'-   #-'୻'-   #
import math
try:
    n3 = 2
    while n3 > 0:
        n3 -= 1
        print(('{}-{!r}-{:>4}'.format(math.ceil(math.atan(-0.0)), chr(2939), (2 + int(0.0))) * (math.ceil(abs(int(+7.125)) % 30) % 4)))
        try:
            print(math.sqrt(7.125))
        finally:
            print("finally", 3 ** 50)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

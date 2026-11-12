# difffuzz divergence, seed 32643
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0cd4']
#   mm:      ['೔']
# signature: CPython: ['\u#cd#'] | mm: ['೔']
import math
try:
    if True:
        print(chr(3284).splitlines())
        def rec14(n):
            if n <= 1:
                return n
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

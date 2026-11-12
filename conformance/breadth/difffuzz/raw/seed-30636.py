# difffuzz divergence, seed 30636
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0ef0', '-', '-', '-', '-', '-'] -
#   mm:      ['໰', '-', '-', '-', '-', '-'] -
# signature: CPython: ['\u#ef#', '-', '-', '-', '-', '-'] - | mm: ['໰', '-', '-', '-', '-', '-'] -
import math
try:
        *a, b = chr(3824).ljust(7, chr(45))
        print(a, b)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

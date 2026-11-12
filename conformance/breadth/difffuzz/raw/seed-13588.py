# difffuzz divergence, seed 13588
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: 717897987691852588770249-'\u0c8d'-  10
#   mm:      717897987691852588770249-'಍'-  10
# signature: CPython: #-'\u#c#d'-  # | mm: #-'಍'-  #
import math
try:
    print('{}-{!r}-{:>4}'.format(3 ** 50, chr(3213).swapcase().title(), 10))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

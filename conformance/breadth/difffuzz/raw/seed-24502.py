# difffuzz divergence, seed 24502
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ["2-'\\u18fd'- 100"]
#   mm:      ["2-'᣽'- 100"]
# signature: CPython: ["#-'\\u#fd'- #"] | mm: ["#-'᣽'- #"]
import math
try:
    v9 = chr(6397)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print('{}-{!r}-{:>4}'.format(2, v9, 100).zfill(0).lstrip().split(','))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

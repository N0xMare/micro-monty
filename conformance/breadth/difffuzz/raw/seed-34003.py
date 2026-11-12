# difffuzz divergence, seed 34003
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v10 ('\u1fb5', '(255,)')
#   mm:      v10 ('᾵', '(255,)')
# signature: CPython: v# ('\u#fb#', '(#,)') | mm: v# ('᾵', '(#,)')
import math
try:
    v10 = (chr(8117), repr((42, 1000, 5, 255, )[3:]), )[:5]
    print('v10', v10)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

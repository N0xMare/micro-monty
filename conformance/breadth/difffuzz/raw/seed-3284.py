# difffuzz divergence, seed 3284
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u0a60': -17}
#   mm:      {'੠': -17}
# signature: CPython: {'\u#a#': -#} | mm: {'੠': -#}
import math
try:
    print({k: v for k, v in zip(sorted(list(chr(2656)), key=lambda w: w[::-1]), sorted({0, 2, 42, 10, -17})) if v})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

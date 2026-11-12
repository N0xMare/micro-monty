# difffuzz divergence, seed 9937
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u177b': -14}
#   mm:      {'᝻': -14}
# signature: CPython: {'\u#b': -#} | mm: {'᝻': -#}
import math
try:
    print({k: v for k, v in zip(list(chr(6011)), list(reversed([a * b for a, b in zip(sorted(sorted({-3, 255, 1, 100, 2}), key=abs), list(map(lambda q: q * -1 + 1, [a * b for a, b in zip([3, 3, 3, 5, 3], [5])])))]))) if v})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

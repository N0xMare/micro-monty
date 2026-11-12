# difffuzz divergence, seed 34247
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u0cb4': -3}
#   mm:      {'಴': -3}
# signature: CPython: {'\u#cb#': -#} | mm: {'಴': -#}
import math
try:
    print({k: v for k, v in zip(chr(3252).splitlines(), list({'k0': -3, 'k1': -3, 'k2': 10, 'k3': 255}.values())) if v})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

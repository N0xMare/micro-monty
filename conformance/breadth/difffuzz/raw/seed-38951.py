# difffuzz divergence, seed 38951
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u083f': 1000}
#   mm:      {'࠿': 1000}
# signature: CPython: {'\u#f': #} | mm: {'࠿': #}
import math
try:
    print(dict(zip(chr(2111).split(','), list(reversed((list(reversed([1000, 1, 255, -1, 255])) * 3))))), sep='Hello World')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 9479
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0c57']
#   mm:      ['౗']
# signature: CPython: ['\u#c#'] | mm: ['౗']
import math
try:
    print(list({chr(3159): pow(int(-0.0), 23, 1000)}))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

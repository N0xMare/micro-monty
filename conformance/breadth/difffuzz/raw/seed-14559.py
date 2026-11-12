# difffuzz divergence, seed 14559
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['-', '1', '\u0c29'] 2
#   mm:      ['-', '1', '఩'] 2
# signature: CPython: ['-', '#', '\u#c#'] # | mm: ['-', '#', '఩'] #
import math
try:
    *a, b = chr(3113).join(sorted([str(q) for q in ([2, -1] * 1)], key=lambda w: w[::-1]))
    print(a, b)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

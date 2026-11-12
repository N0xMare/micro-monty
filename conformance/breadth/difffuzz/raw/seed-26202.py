# difffuzz divergence, seed 26202
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['ß', '\u0a44'] ß
#   mm:      ['ß', '੄'] ß
# signature: CPython: ['ß', '\u#a#'] ß | mm: ['ß', '੄'] ß
import math
try:
    *a, b = chr(2628).split(None)[-1].replace(str(7).join(sorted([], key=lambda w: (-len(w), w)))[6:1:2], '\u00df')
    print(a, b)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

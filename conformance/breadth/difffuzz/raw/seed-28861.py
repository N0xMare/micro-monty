# difffuzz divergence, seed 28861
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v5 ['\u245a', '\u245a', '\u245a']
#   mm:      v5 ['⑚', '⑚', '⑚']
# signature: CPython: v# ['\u#a', '\u#a', '\u#a'] | mm: v# ['⑚', '⑚', '⑚']
import math
try:
    v5 = list((chr(9306) * (math.isqrt(abs(10) % 30) % 4)).replace('  pad  '.removesuffix('123').join(['AbC']), 'x\ny'))
    print('v5', v5)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

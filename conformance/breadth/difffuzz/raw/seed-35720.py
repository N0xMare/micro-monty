# difffuzz divergence, seed 35720
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v27 {'-42': 1, '\u218f': 100, '  pad  ': 2}
#   mm:      v27 {'-42': 1, '↏': 100, '  pad  ': 2}
# signature: CPython: v# {'-#': #, '\u#f': #, '  pad  ': #} | mm: v# {'-#': #, '↏': #, '  pad  ': #}
import math
try:
    v27 = {'-42': int(1.5), chr(8591): {'k0': -17, 'k1': 1, 'k2': 100}['k2'], '  pad  ': round(((1.5 ** 2) ** 0.5))}
    print('v27', v27)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

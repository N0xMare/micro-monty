# difffuzz divergence, seed 37392
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v10 {'\u17ed': 1}
#   mm:      v10 {'៭': 1}
# signature: CPython: v# {'\u#ed': #} | mm: v# {'៭': #}
import math
try:
    v10 = {k11: len(k11) for k11 in chr(6125).upper().splitlines()}
    print('v10', v10)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 14263
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v27 {'\u1aeb': 0}
#   mm:      v27 {'᫫': 0}
# signature: CPython: v# {'\u#aeb': #} | mm: v# {'᫫': #}
import math
try:
    v27 = dict(sorted(dict(sorted({chr(6891): round(0.1)}.items(), key=lambda kv: kv[1])).items(), key=lambda kv: kv[1]))
    print('v27', v27)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

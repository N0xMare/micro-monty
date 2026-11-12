# difffuzz divergence, seed 38281
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '***\u0c52***'
#   mm:      EXC ValueError invalid literal for int() with base 10: '***౒***'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '***\u#c#***' | mm: EXC ValueError invalid literal for int() with base #: '***౒***'
import math
try:
    def f1(p0):
        v2 = set([i + v for i, v in enumerate((sorted([2]) + list(reversed([1000, 10, 255]))), 2)])
        return int(chr(3154).center(7, chr(42)), 10)
    print("f1('-42')", f1('-42'))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

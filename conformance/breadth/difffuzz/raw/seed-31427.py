# difffuzz divergence, seed 31427
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0a29'
#   mm:      EXC ValueError could not convert string to float: '਩'
# signature: CPython: EXC ValueError could not convert string to float: '\u#a#' | mm: EXC ValueError could not convert string to float: '਩'
import math
try:
    v3 = chr(2601)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(list(({k18: len(k18) for k18 in ['\U0001f600 ok', 'AbC', '\u00df']} | {'x\ny': 42}).values())[divmod(math.trunc(float(v3)), ((-3 ** (7 % 7)) if isinstance(v3, bool) else sum(lst0)))[1]], sep='mississippi')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

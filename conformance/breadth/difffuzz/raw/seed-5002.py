# difffuzz divergence, seed 5002
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC KeyError 'k2'
#   mm:      EXC MemoryError 
# signature: CPython: EXC KeyError 'k#' | mm: EXC MemoryError 
import math
try:
    print(divmod((2 ** 64 ** (pow(int('-42', 0), 27, 2 ** 61 - 1) % 7)), {'k0': 1}['k2'])[0])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

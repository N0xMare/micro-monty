# difffuzz divergence, seed 24704
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1ad1'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᫑'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ad#' | mm: EXC ValueError invalid literal for int() with base #: '᫑'
import math
try:
    print((tuple(sorted({'k0': 1}.items())) + divmod(pow(pow(3, 29, 7), 39, 7), int(chr(6865)))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

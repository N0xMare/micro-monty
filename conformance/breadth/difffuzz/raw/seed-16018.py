# difffuzz divergence, seed 16018
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u2cf8'
#   mm:      EXC ValueError invalid literal for int() with base 16: '⳸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cf#' | mm: EXC ValueError invalid literal for int() with base #: '⳸'
import math
try:
    v7 = list({'caf\u00e9': int(chr(11512), 16), repr(0.1): 7})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

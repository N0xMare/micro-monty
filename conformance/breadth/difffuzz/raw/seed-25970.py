# difffuzz divergence, seed 25970
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2fd7'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⿗'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fd#' | mm: EXC ValueError invalid literal for int() with base #: '⿗'
import math
try:
    v13[(f'{0.1:.0f}|'.expandtabs() * (max(list(range(3, -4, 2))) % 4))] = int(chr(12247).title().zfill(1))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 25314
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0bd4'
#   mm:      EXC ValueError invalid literal for int() with base 16: '௔'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#bd#' | mm: EXC ValueError invalid literal for int() with base #: '௔'
import math
try:
    try:
        print(int(chr(3028), 16) // len(('AbC'.split(' ')[-1] + '%e' % ('hello', i14,).partition(' ')[1])))
    except IndexError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 16569
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u0605'
#   mm:      EXC ValueError invalid literal for int() with base 0: '؅'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '؅'
import math
try:
    def f28(p0, *rest, kw=2):
        return int('{}-{!r}-{:>4}'.format('caf\u00e9'.lower().index(''.partition(' ')[1]), '{}-{!r}-{:>4}'.format(round(1e10), repr(['a,b,,c']), i22), math.trunc(1e-05)), 36)
    print("f28(int(chr(1541).split(' ')[-1], 0))", f28(int(chr(1541).split(' ')[-1], 0)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 4461
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u1ae7'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᫧'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ae#' | mm: EXC ValueError invalid literal for int() with base #: '᫧'
import math
try:
    v10 = ('\u00df' != 'AbC'.replace('Hello World', '-42').join(list('caf\u00e9')).zfill(9))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v34 = str({str(q): q for q in list(reversed([len(w) for w in ['x\ny']]))})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(*list(reversed(sorted(list(filter(lambda q: q > 2, list(reversed([-17, 1])))), key=abs))), *'{}-{!r}-{:>4}'.format(str(v10).partition(' ')[0].rfind(v34.split(None)[0].replace(v34, v34.rjust(1, chr(45)))), v34, math.factorial(abs(int(chr(6887), 0)) % 30)), sep=",")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

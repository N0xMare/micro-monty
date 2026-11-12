# difffuzz divergence, seed 7003
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2e6c'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⹬'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#c' | mm: EXC ValueError invalid literal for int() with base #: '⹬'
import math
try:
    *a, b = str(int(chr(11884), 10)).replace(('{}-{!r}-{:>4}'.format(-1, 'tab\there', 100) + 'AbC'.join(v4)).replace('a,b,,c'.replace('x\ny', '123').split(' ')[1], ('tab\there' + 'hello').upper()), f'{None!s}|{[-17, 1000, 0, 42]}|{1e300:%}|'.title())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

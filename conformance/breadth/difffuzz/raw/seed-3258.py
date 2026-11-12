# difffuzz divergence, seed 3258
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0e6f'
#   mm:      EXC ValueError invalid literal for int() with base 16: '๯'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#f' | mm: EXC ValueError invalid literal for int() with base #: '๯'
import math
try:
    v1 = {str(q): q for q in [len(w) for w in '\u00df'.join(['3.5', '\u00df', 'a', 'a']).splitlines()]}
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v1.update({'\u00df'.join(list({'AbC': 5})): int(chr(3695).partition(' ')[0], 16)})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

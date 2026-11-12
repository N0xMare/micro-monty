# difffuzz divergence, seed 14992
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u175d'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᝝'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d' | mm: EXC ValueError invalid literal for int() with base #: '᝝'
import math
try:
    try:
        print(((7, 255, 10, 2, ) + divmod(i1, 0)[:-5])[:1][divmod((min(list(map(lambda q: q * -2 + 1, []))) >> (int(('hello' + '')) % 9)), int(f'{1e10:G} {-17:.2f} ', 0))[1]])
    finally:
        print("finally", int(chr(5981)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

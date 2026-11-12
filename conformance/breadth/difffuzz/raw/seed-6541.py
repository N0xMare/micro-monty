# difffuzz divergence, seed 6541
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u171e'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᜞'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e' | mm: EXC ValueError invalid literal for int() with base #: '᜞'
import math
try:
    v19 = {repr('123'.lower()).replace('x\ny'.expandtabs().join(sorted(['\u00df', '\u00df', 'a,b,,c', 'Hello World'], key=lambda w: (-len(w), w))), (('tab\there' * (255 % 4)) * (42 % 4))): int(chr(5918).strip('3.5'.join([])), 10)}
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

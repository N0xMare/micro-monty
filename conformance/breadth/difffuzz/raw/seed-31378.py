# difffuzz divergence, seed 31378
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1774'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᝴'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '᝴'
import math
try:
    v13 = {'a'.replace('  pad  '.removeprefix('  pad  '), 'tab\there'[5:]).split('s')[0]: int(chr(6004)), '\u00df'.rjust(2, chr(42)).replace('a'.strip('3.5'), 'AbC'.replace('123', 'caf\u00e9'))[:6]: int(repr(+3.0)), chr(1704).ljust(3, chr(45)): (min([ord(c) for c in 'tab\there']) if ('caf\u00e9' + 'mississippi').isalnum() else {str(q): q for q in lst0}['k1'])}
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

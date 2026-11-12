# difffuzz divergence, seed 12751
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '0\u05ce\u05ce\u05ce'
#   mm:      EXC ValueError invalid literal for int() with base 36: '0׎׎׎'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#ce\u#ce\u#ce' | mm: EXC ValueError invalid literal for int() with base #: '#׎׎׎'
import math
try:
    v7 = int((chr(1486) * ('tab\there'.find('  pad  ') % 4)).zfill(4), 36)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

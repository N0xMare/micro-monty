# difffuzz divergence, seed 33933
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2d770\u2d770\u2d77ß\u2d77']
#   mm:      ['⵷0⵷0⵷ß⵷']
# signature: CPython: ['\u#d#\u#d#\u#d#ß\u#d#'] | mm: ['⵷#⵷#⵷ß⵷']
import math
try:
    data = '\u00df'.zfill(5).replace('\U0001f600 ok'.join(['']), chr(11639))[4:].split(' ')
    print(sorted(data, key=len, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

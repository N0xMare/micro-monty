# difffuzz divergence, seed 31352
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v1 ['\u2d9eA\u2d9e', '\u2d9eB\u2d9e', '\u2d9e', '\u2d9eC\u2d9e']
#   mm:      v1 ['⶞A⶞', '⶞B⶞', '⶞', '⶞C⶞']
# signature: CPython: v# ['\u#d#eA\u#d#e', '\u#d#eB\u#d#e', '\u#d#e', '\u#d#eC\u#d#e'] | mm: v# ['⶞A⶞', '⶞B⶞', '⶞', '⶞C⶞']
import math
try:
    v1 = 'a,b,,c'.title().replace(('' * (10 % 4)), chr(11678)).rjust(3, chr(42)).split(',')
    print('v1', v1)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

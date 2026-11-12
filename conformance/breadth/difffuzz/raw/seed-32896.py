# difffuzz divergence, seed 32896
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u19ad\u19ad': 223}
#   mm:      {'᦭᦭': 223}
# signature: CPython: {'\u#ad\u#ad': #} | mm: {'᦭᦭': #}
import math
try:
    print({k: v for k, v in zip(sorted((chr(6573) * (sum([1, -3]) % 4)).splitlines(), key=str.lower), ([len(w) for w in []] + ([ord(c) for c in '\U0001f600 ok'.partition(',')[2]] + [ord(c) for c in '\u00df']))) if v})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

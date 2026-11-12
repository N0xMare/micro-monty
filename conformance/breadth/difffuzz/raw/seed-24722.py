# difffuzz divergence, seed 24722
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0b340000000ß']
#   mm:      ['଴0000000ß']
# signature: CPython: ['\u#b#ß'] | mm: ['଴#ß']
import math
try:
        data = (chr(2868).swapcase() + '\u00df'.removeprefix('-42').zfill(8)).split(',')
        print(sorted(data, key=lambda q: str(q)[::-1], reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

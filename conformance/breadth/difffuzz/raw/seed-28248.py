# difffuzz divergence, seed 28248
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u05f5']
#   mm:      ['׵']
# signature: CPython: ['\u#f#'] | mm: ['׵']
import math
try:
    n9 = 3
    while n9 > 0:
        data = chr(1525)[::1].replace(('caf\u00e9'[::-2] + f'{[3, 100, -17]!r}{0.1:+.2f}{n9:08b}|'), repr(['tab\there', 'x\ny'])).split()
        print(sorted(data, key=lambda q: str(q)[::-1], reverse=True))
        data = (str((1000, -1, )) + ('x\ny' * (n9 % 4)))[-[-3, 255][n9]].splitlines()
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

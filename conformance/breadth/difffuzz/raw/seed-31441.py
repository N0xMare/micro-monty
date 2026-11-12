# difffuzz divergence, seed 31441
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v11 ['2', '-', "'", 'T', 'r', 'u', 'e', "'", '-', ' ', ' ', ' ', '0', '\u0ad8']
#   mm:      v11 ['2', '-', "'", 'T', 'r', 'u', 'e', "'", '-', ' ', ' ', ' ', '0', '૘']
# signature: CPython: v# ['#', '-', "'", 'T', 'r', 'u', 'e', "'", '-', ' ', ' ', ' ', '#', '\u#ad#'] | mm: v# ['#', '-', "'", 'T', 'r', 'u', 'e', "'", '-', ' ', ' ', ' ', '#', '૘']
import math
try:
    v11 = list(('{}-{!r}-{:>4}'.format(2, str(True), pow(1000, 39, -5)) + chr(2776)))
    print('v11', v11)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

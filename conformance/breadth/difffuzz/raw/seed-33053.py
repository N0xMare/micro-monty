# difffuzz divergence, seed 33053
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['1', '2', '3', '\u0ad4', '3', '.', '5', '\u0ad4', 'A', 'b', 'C'] ૔
#   mm:      ['1', '2', '3', '૔', '3', '.', '5', '૔', 'A', 'b', 'C'] ૔
# signature: CPython: ['#', '#', '#', '\u#ad#', '#', '.', '#', '\u#ad#', 'A', 'b', 'C'] ૔ | mm: ['#', '#', '#', '૔', '#', '.', '#', '૔', 'A', 'b', 'C'] ૔
import math
try:
    *a, b = (chr(2772).join(['123', '3.5', 'AbC', '']) + 'mississippi'.zfill(0).partition(',')[2].lstrip())
    print(a, b)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

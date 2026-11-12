# difffuzz divergence, seed 10903
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u1ae2', '-', '2', '2', '5', '.', '0', '%', '|', '-', '2', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', '1', ' ']
#   mm:      ['᫢', '-', '2', '2', '5', '.', '0', '%', '|', '-', '2', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', '1', ' ']
# signature: CPython: ['\u#ae#', '-', '#', '#', '#', '.', '#', '%', '|', '-', '#', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', | mm: ['᫢', '-', '#', '#', '#', '.', '#', '%', '|', '-', '#', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', '#',
import math
try:
    print(list((chr(6882) + f'{-2.25:.1%}|{-2.25:.0f} {1:>8} ')))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

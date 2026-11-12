# difffuzz divergence, seed 8367
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2fe3']3.5
#   mm:      ['⿣']3.5
# signature: CPython: ['\u#fe#']#.# | mm: ['⿣']#.#
import math
try:
    print(list(chr(12259)), end='3.5')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

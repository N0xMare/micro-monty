# difffuzz divergence, seed 32859
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: v11 (3, 'ß', '\u2d76')
#   mm:      v11 (3, 'ß', '⵶')
# signature: CPython: v# (#, 'ß', '\u#d#') | mm: v# (#, 'ß', '⵶')
import math
try:
    n4 = 1
    while n4 > 0:
        n5 = 6
        while n5 > 0:
            try:
                print({'k0': 42, 'k1': 100}['123'.split(',')[1].swapcase()])
            except ValueError as e:
                print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
    v11 = repr((abs(n5).bit_length(), '\u00df'.expandtabs(), chr(11638), )).rjust(0, chr(45))
    print('v11', v11)

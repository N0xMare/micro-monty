# difffuzz divergence, seed 26240
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2fd6']ß['\u2fd6']ß['\u2fd6']ßn6 0
#   mm:      ['⿖']ß['⿖']ß['⿖']ßn6 0
# signature: CPython: ['\u#fd#']ß['\u#fd#']ß['\u#fd#']ßn# # | mm: ['⿖']ß['⿖']ß['⿖']ßn# #
import math
try:
    def f4():
        n5 = 3
        while n5 > 0:
            n5 -= 1
            n6 = 3
            while n6 > 0:
                n6 -= 1
                print(sorted(list(chr(12246)), key=str.lower), end='\u00df')
            print('n6', n6)
    print('f4()', f4())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

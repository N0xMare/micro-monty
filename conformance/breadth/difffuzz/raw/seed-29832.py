# difffuzz divergence, seed 29832
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: '\u1757'😀 okEXC AttributeError 'tuple' object has no attribute 'isalpha'
#   mm:      '᝗'😀 okEXC AttributeError 'tuple' object has no attribute 'isalpha'
# signature: mm: '᝗'😀 okEXC AttributeError 'tuple' object has no attribute 'isalpha'
import math
try:
    print(repr(chr(5975)), end='\U0001f600 ok')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    if '%d' % (f'{[]!r}|',).isalpha():
        print([ord(c) for c in 'hello'.removesuffix(f'{5:<6} and {n12:10.3e} '[{'k0': 7}['k1']])])
        match (1, "x"):
            case 'a' | 'hello':
                print("word")
        for i20 in [a * b for a, b in zip([len(w) for w in ['  pad  ', '-42']], [a * b for a, b in zip([len(w) for w in v14], sorted([255, 3, -1, 0], key=lambda q: (q % 3, -q)))][:0])]:
            if i20 == 1:
                continue
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

# difffuzz divergence, seed 10736
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2e65'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⹥'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '⹥'
import math
try:
    v1 = ('Hello World'.rstrip() * (-3 % 4)).partition(' ')[1][abs(divmod(sum([3, 2, 10, 3, -1]), -17)[1]).bit_length()]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C5:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C5((-(2 ** 70) if ((v1 * (10 % 4)).title() in list(v1.join(['a', 'a,b,,c']))) else int(chr(11877))), -3), C5(len([str(q) for q in list(reversed(list(range(3, 5, -1))))]), 4), C5(({str(q): q for q in [len(w) for w in ['mississippi']]} | dict(zip(v1.split(','), [a * b for a, b in zip(lst0, lst0)])))['k1'], 3), C5.make(1)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

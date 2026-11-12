# difffuzz divergence, seed 15267
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC IndexError cannot fit 'int' into an index-sized integer
#   mm:      EXC MemoryError 
# signature: CPython: EXC IndexError cannot fit 'int' into an index-sized integer | mm: EXC MemoryError 
import math
try:
    class C2:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C2(([a * b for a, b in zip(list(range(-2, -2, -1)), sorted({42, 0, 3}))][(2 ** 64 ** (-(2 ** 70) % 7))] >> (len(list('  pad  '.partition(' ')[1])) % 9)), 3), C2(sum(list({}.values())), -3), C2('tab\there'.center(11, chr(42)).replace('mississippi'.center(9, chr(42)), str([10, 2, 0]))[:1].index(repr(tuple(list(filter(lambda q: q > 2, [100, -1, -17]))))), -1), C2.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

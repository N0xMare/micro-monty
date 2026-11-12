# difffuzz divergence, seed 33486
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC TypeError '<' not supported between instances of 'str' and 'int'
#   mm:      EXC TypeError '<' not supported between instances of 'int' and 'str'
# signature: mm: EXC TypeError '<' not supported between instances of 'int' and 'str'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.v = v
            self.w = w
        def __repr__(self):
            return f'C1({self.v!r}, {self.w})'
        def __lt__(self, other):
            return (self.v, self.w) < (other.v, other.w)
        def make(n):
            return C1(n, -n)
    objs = [C1(pow(('Hello World' * (100 % 4)).join(list({})).rfind(str(False).partition('s')[2]), 31, 7), 0), C1(abs(len(set(list(range(-2, 2, 1))))).bit_length(), 0), C1('%e' % ('a'.partition(',')[1], chr(9266),).count('{}-{!r}-{:>4}'.format(7, 'mississippi', 3).partition(',')[0].partition('s')[2]), 4), C1.make(3)]
    print(objs, sorted(objs), max(objs), min(objs, key=lambda o: o.w))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

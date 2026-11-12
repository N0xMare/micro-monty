# difffuzz divergence, seed 41139
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC TypeError '<' not supported between instances of 'str' and 'int'
#   mm:      EXC TypeError '<' not supported between instances of 'int' and 'str'
# signature: mm: EXC TypeError '<' not supported between instances of 'int' and 'str'
import math
try:
    v3 = int(float(1))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C11:
        def __init__(self, v, w=0):
            self.v = v
            self.w = w
        def __repr__(self):
            return f'C11({self.v!r}, {self.w})'
        def __lt__(self, other):
            return (self.v, self.w) < (other.v, other.w)
        def make(n):
            return C11(n, -n)
    objs = [C11((sum(sorted({s12 % 3 for s12 in lst0})) * divmod(v3, (math.ceil(2.5) if isinstance(lst0, bool) else len((-17, 100, ))))[1]), 1), C11((round(1.5) >> (math.ceil(abs(int(abs(1e-05))) % 30) % 9)), 2), C11('%05d' % (-0.0,).count('mississippi'), -2), C11.make(0)]
    print(objs, sorted(objs), max(objs), min(objs, key=lambda o: o.w))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

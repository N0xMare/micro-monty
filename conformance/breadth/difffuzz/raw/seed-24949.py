# difffuzz divergence, seed 24949
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: [1]EXC TypeError '<' not supported between instances of 'str' and 'int'
#   mm:      [1]EXC TypeError '<' not supported between instances of 'int' and 'str'
# signature: mm: [#]EXC TypeError '<' not supported between instances of 'int' and 'str'
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(sorted((({10, 42, -3, 1, 3} & set(list(map(lambda q: q * 0 + 1, lst0)))) - {5})), end='')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C9:
        def __init__(self, v, w=0):
            self.v = v
            self.w = w
        def __repr__(self):
            return f'C9({self.v!r}, {self.w})'
        def __lt__(self, other):
            return (self.v, self.w) < (other.v, other.w)
        def make(n):
            return C9(n, -n)
    objs = [C9(sum([i + v for i, v in enumerate((list(filter(lambda q: q > 0, lst0)) + sorted(lst0, reverse=True)), 1)]), 0), C9(~sum(sorted({s10 % 2 for s10 in lst0})), 0), C9('%-6s|' % ('  pad  ',)[pow(-1, 29, -5)].ljust(12, chr(45)).count('3.5'.removeprefix((str(True) * ((3 ** (-17 % 7)) % 4)))), -3), C9.make(5)]
    print(objs, sorted(objs), max(objs), min(objs, key=lambda o: o.w))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)

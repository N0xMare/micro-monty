# iter(callable, sentinel) with a sentinel that compares in Python
# (Cont::CallIterEq): CPython compares `sentinel == value`.
class P:
    def __init__(self, v):
        self.v = v
    def __eq__(self, o):
        print('eq', self.v, getattr(o, 'v', o))
        return isinstance(o, P) and self.v == o.v
    def __repr__(self):
        return f'P({self.v})'
    __hash__ = None
def make(vals):
    it = iter(vals)
    return lambda: next(it)
print(list(iter(make([P(1), P(2), P(0), P(3)]), P(0))))
it = iter(make([P(5), P(0)]), P(0))
print(next(it), next(it, 'done'), next(it, 'done'))
for x in iter(make([1, P(4), P(0), 7]), P(0)):
    print('got', x)
print(sorted(iter(make([P(2), P(1), P(0)]), P(0)), key=lambda p: p.v))
class Truthy:
    def __init__(self, b): self.b = b
    def __bool__(self): return self.b
class Q:
    def __init__(self, v): self.v = v
    def __eq__(self, o): return Truthy(self.v == 0)
    __hash__ = None
print(len(list(iter(make([Q(1), Q(2), Q(0), Q(4)]), Q(0)))))
class Boom:
    def __eq__(self, o): raise RuntimeError('boom')
    __hash__ = None
try:
    list(iter(make([Boom()]), 0))
except RuntimeError as e:
    print('RuntimeError', e)
print(list(iter(make([[0], [1], [0, 1], [0]]), [0, 1])))

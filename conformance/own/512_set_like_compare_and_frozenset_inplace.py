# Set-like comparisons of dict views with 65,536 items or more once
# raised TypeError: the containment loop gathered the left operand a chunk
# at a time and handed the twin a list. A set compared with a set
# subclass's instance on the right compared as an unordered value, and an
# in-place operator on a frozenset with a set subclass's instance raised.
import operator

N = 65536


class SS(set):
    pass


class K:
    def __init__(self, v):
        self.v = v

    def __hash__(self):
        return hash(self.v)

    def __eq__(self, other):
        return isinstance(other, K) and other.v == self.v


d = dict.fromkeys(range(N))
e = dict.fromkeys(range(N + 1))
s = set(range(N))
print(d.keys() <= e.keys(), e.keys() >= d.keys(), d.keys() < e.keys(), e.keys() <= d.keys())
print(d.keys() <= set(e), s >= d.keys(), set(d.items()) <= e.items(), d.items() <= e.items())

for name in ('lt', 'le', 'gt', 'ge', 'eq', 'ne'):
    f = getattr(operator, name)
    print(name, [f(a, b) for a, b in (({1}, SS([1, 2])), (SS([1, 2]), {1}), (frozenset([1]), SS([1])),
                                       (SS([1]), SS([1, 3])), ({1}, SS([1])))])

for op in ('|=', '&=', '-=', '^='):
    fs = frozenset([0, 64])
    old = fs
    x = SS([K(0), 5, 0])
    if op == '|=':
        fs |= x
    elif op == '&=':
        fs &= x
    elif op == '-=':
        fs -= x
    else:
        fs ^= x
    print(op, type(fs).__name__, fs is old, len(fs), len(old), hash(old) == hash(frozenset([0, 64])))

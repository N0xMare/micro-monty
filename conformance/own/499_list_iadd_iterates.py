# A list's `+=` takes its operand as `extend` does: only an exact list or
# tuple, and a dict's keys, are read in place; a subclass's own `__iter__`
# runs. A list subclass's `+=` extends it in place, from any iterable.
class LI(list):
    def __iter__(self):
        return iter([9, 9])


class TI(tuple):
    def __iter__(self):
        return iter([8])


class DI(dict):
    def __iter__(self):
        return iter(['k'])


class LP(list):
    pass


class L(list):
    pass


for x in (LI([1, 2, 3]), TI((1, 2)), DI(a=1, b=2), LP([4, 5]), (6, 7), {'d': 0}):
    y = []
    y += x
    z = []
    z.extend(x)
    w = L([0])
    before = w
    w += x
    print(type(x).__name__, y, z, w, type(w).__name__, w is before)

w = L()
w += (i for i in range(3))
print(w, type(w).__name__)
w += 'ab'
print(w)
try:
    w += 5
except TypeError as e:
    print('TypeError', e)
print(w)


# `a += a` and `a.extend(a)` read the list's own items, not a subclass's
# `__iter__`.
class LN(list):
    n = 0

    def __iter__(self):
        LN.n += 1
        return iter([9])


a = LN([1, 2])
y = a
a += a
print(a, a is y, LN.n)
b = LN([3])
b.extend(b)
print(b, LN.n)


# A list subclass's `+=` runs the `__iadd__` it inherits, which raises for
# a non-iterable: the right operand's `__radd__` and the subclass's own
# `__add__` do not run. An exact list's `+=` falls back to `__radd__`.
class RA:
    def __radd__(self, o):
        return 'radd'


class LA(list):
    def __add__(self, o):
        return 'LA.add'


for make in (lambda: [1], lambda: L([1]), lambda: LA([1])):
    x = make()
    try:
        x += RA()
        print(type(make()).__name__, 'ok', x)
    except TypeError as e:
        print(type(make()).__name__, 'TypeError', e)
x = LA([1])
x += [2]
print(type(x).__name__, x, LA([1]) + [2])

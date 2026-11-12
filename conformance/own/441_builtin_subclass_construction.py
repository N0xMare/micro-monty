# Builtin-type subclasses built through their own `__new__` (the builtin
# `__init__` still fills them), set subclasses' repr naming the class,
# object.__init__'s excess-argument message, and frozenset's slot wrappers.
class L(list):
    def __new__(cls, *a):
        return super().__new__(cls)


class D(dict):
    def __new__(cls, *a, **k):
        return super().__new__(cls)


class S(set):
    def __new__(cls, *a):
        return super().__new__(cls)


print(L([1, 2]), type(L()).__name__, L())
print(D(a=1), D([(1, 2)], b=3), D())
print(sorted(S([3, 1])), S(), S('a'))


class L2(list):
    def __new__(cls, *a):
        return super().__new__(cls)

    def __init__(self, *a):
        super().__init__(['x'])


print(L2([1, 2]))
try:
    L(5)
except TypeError as e:
    print(e)


class P(set):
    pass


class R:
    def __repr__(self):
        return 'R!'


print(P(), P([1]), [P([2])], str(P()), f'{P([3])!r}', P([R()]), [P([R()])])
s = P([1])
s.add(2)
print(s, repr(s) == str(s))


class A:
    def __init__(self, x):
        super().__init__(x)


class B:
    pass


class C:
    def __new__(cls, *a):
        return super().__new__(cls)


for f in (lambda: A(1), lambda: object.__init__(object(), 1), lambda: object.__init__(B(), 1),
          lambda: B(1), lambda: C(1, 2)):
    try:
        print(type(f()).__name__)
    except TypeError as e:
        print(e)

f = frozenset({1, 2})
print(f.__hash__() == hash(f), f.__eq__(frozenset({1, 2})), f.__eq__({1, 2}), {1}.__eq__(frozenset({1})))
print(f.__len__(), f.__contains__(1), sorted(f.__iter__()), f.__repr__(), f.__ne__(f), f.__ne__(1))

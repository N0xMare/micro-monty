# A class's metaclass is the most derived of its explicit metaclass and its
# bases' metaclasses; when none of them is a subclass of all the others,
# the class statement, `type(name, bases, ns)` and a metaclass's
# `type.__new__` raise TypeError, as CPython's `_PyType_CalculateMetaclass`.
# A metaclass that is not a class is called as is.

CONFLICT = ('metaclass conflict: the metaclass of a derived class must be a (non-strict) '
            'subclass of the metaclasses of all its bases')


class M1(type):
    pass


class M2(type):
    pass


class M3(M1):
    pass


class A(metaclass=M1):
    pass


class B(metaclass=M2):
    pass


class C(metaclass=M3):
    pass


def conflict(f):
    try:
        f()
    except TypeError as e:
        return str(e) == CONFLICT
    return False


def stmt_ab():
    class X(A, B):
        pass


def stmt_ba():
    class X(B, A):
        pass


def stmt_explicit():
    class X(A, metaclass=M2):
        pass


assert conflict(stmt_ab) and conflict(stmt_ba) and conflict(stmt_explicit)
assert conflict(lambda: type('X', (A, B), {})) and conflict(lambda: M2('X', (A,), {}))


# The most derived wins, wherever it is among the bases.
class D(A, C):
    pass


class E(C, A, metaclass=M1):
    pass


class P:
    pass


class F(P, A):
    pass


assert type(D) is M3 and type(E) is M3 and type(F) is M1
assert type(type('X', (A, C), {})) is M3 and type(M1('X', (C,), {})) is M3


def factory(name, bases, ns):
    return (name, bases)


class G(A, B, metaclass=factory):
    pass


assert G == ('G', (A, B))
print('ok')

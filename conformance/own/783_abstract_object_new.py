# `object.__new__` of a class with abstract methods raises CPython's
# TypeError, also when a class's own `__new__` calls it through `super()`;
# a class with none left, one whose set was emptied, and an exception
# class (its own `__new__`) are made.
import abc


def refused(f):
    try:
        f()
    except TypeError as e:
        return str(e)
    raise AssertionError('made')


class P(abc.ABC):
    @abc.abstractmethod
    def m(self): ...

    @abc.abstractmethod
    def a(self): ...


class Q(P):
    def m(self):
        return 1


class R(Q):
    def a(self):
        return 2


assert refused(lambda: P.__new__(P)) == (
    "Can't instantiate abstract class P without an implementation for abstract methods 'a', 'm'")
assert refused(lambda: object.__new__(Q)) == (
    "Can't instantiate abstract class Q without an implementation for abstract method 'a'")
assert type(object.__new__(R)) is R and R().a() == 2


class N:
    def __new__(cls, *a):
        return super().__new__(cls)


class NA(N, abc.ABC):
    @abc.abstractmethod
    def z(self): ...


assert refused(lambda: NA.__new__(NA)).endswith("method 'z'")
assert type(N(1)) is N


class E(ValueError, abc.ABC):
    @abc.abstractmethod
    def q(self): ...


assert type(E.__new__(E)) is E
P.__abstractmethods__ = frozenset()
assert type(P.__new__(P)) is P

# A protocol and an ABC as bases of one class: CPython's protocol metaclass
# derives from `ABCMeta`, so the class is built with it; micro-monty's
# derives from `type` and refuses the class with CPython's metaclass-conflict
# TypeError instead of picking one metaclass (docs/user-guide.md). Both
# print the same lines. A protocol alone, or beside a plain class, builds.
import abc
from typing import Protocol, runtime_checkable


@runtime_checkable
class Sized(Protocol):
    def __len__(self): ...


def mixed(*bases):
    try:
        c = type('C', bases, {'__len__': lambda self: 1})
    except TypeError as e:
        return str(e).startswith('metaclass conflict: ')
    return isinstance(c(), Sized) and isinstance(c(), abc.ABC)


print('protocol, ABC handled:', mixed(Sized, abc.ABC))
print('ABC, protocol handled:', mixed(abc.ABC, Sized))


class Base:
    pass


class C(Base, Sized):
    def __len__(self):
        return 2


print(type(C).__name__, isinstance(C(), Sized), len(C()))

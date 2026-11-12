# type(name, bases, ns) with a base whose metaclass is not `type` builds the
# class through that metaclass (CPython's winning metaclass): ABCMeta then
# sees the namespace's implementations, and a user metaclass's __new__ and
# __init__ run with any class keywords.
from collections.abc import Sized, Mapping
import abc

C = type('C', (Sized,), dict.fromkeys(['__len__'], lambda s, *a: 0))
print(len(C()), C.__abstractmethods__, isinstance(C(), Sized))
D = type('D', (Mapping,), dict.fromkeys(['__getitem__', '__iter__', '__len__'], lambda s, *a: 0))
print(type(D()).__name__, sorted(D.__abstractmethods__))
try:
    type('P', (Mapping,), {'__len__': lambda s: 0})()
except TypeError as e:
    print(e)

class A(abc.ABC):
    @abc.abstractmethod
    def f(self): ...
E = type('E', (A,), {'f': lambda s: 1})
print(E().f(), type(E).__name__)

class Meta(type):
    def __new__(mcls, name, bases, ns, **kw):
        ns['tag'] = kw.get('tag', 'none')
        print('Meta.__new__', name, sorted(k for k in ns if not k.startswith('__')))
        return super().__new__(mcls, name, bases, ns)
    def __init__(cls, name, bases, ns, **kw):
        print('Meta.__init__', name)
        super().__init__(name, bases, ns)
class Base(metaclass=Meta):
    pass
K = type('K', (Base,), {'x': 1}, tag='t')
print(type(K).__name__, K.tag, K.x, K.__mro__[1].__name__)
print(type(type('Plain', (), {})).__name__, type('Q', (int,), {})(5) + 1)

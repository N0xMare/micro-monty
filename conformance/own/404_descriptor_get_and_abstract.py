# The descriptor protocol on builtin descriptors: functions, classmethods,
# staticmethods and properties have `__get__` (a decorator class can bind
# the function it wraps), and classmethod/staticmethod/property have
# `__isabstractmethod__`; functools.partialmethod and singledispatchmethod
# bind through `__get__`, so they wrap classmethods as CPython's do.
def f(*a): return a
cm, sm, pr = classmethod(f), staticmethod(f), property(f)
print(hasattr(cm, '__get__'), hasattr(sm, '__get__'), hasattr(pr, '__get__'), hasattr(f, '__get__'))
class A: pass
print(cm.__get__(None, A)(), sm.__get__(None, A)(1), f.__get__(A())() [0].__class__.__name__)
print(pr.__get__(A(), A)[0].__class__.__name__, pr.__get__(None, A) is pr)
print(cm.__func__ is f, sm.__func__ is f, cm.__wrapped__ is f, hasattr(cm, '__isabstractmethod__'))
import abc, functools
class Base(abc.ABC):
    @classmethod
    @abc.abstractmethod
    def make(cls): ...
    @property
    @abc.abstractmethod
    def size(self): ...
    @staticmethod
    @abc.abstractmethod
    def util(): ...
print(sorted(Base.__abstractmethods__), Base.__dict__['size'].__isabstractmethod__, cm.__isabstractmethod__)
class Impl(Base):
    @classmethod
    def make(cls): return cls()
    size = property(lambda self: 3)
    util = staticmethod(lambda: 1)
print(Impl.make().size, Impl.util())
class Logged:
    def __init__(self, fn): self.fn = fn
    def __get__(self, obj, objtype=None):
        bound = self.fn.__get__(obj, objtype)
        def call(*a): return ("logged", bound(*a))
        return call
class K:
    @Logged
    def m(self, x): return x * 2
    @Logged
    @classmethod
    def c(cls): return cls.__name__
print(K().m(4), K().c(), K.c())
class D:
    @functools.singledispatchmethod
    @classmethod
    def t(cls, arg): return "base"
    @t.register(int)
    @classmethod
    def _(cls, arg): return "int"
print(D.t(1), D.t("x"), D().t(2))
class P:
    @functools.partialmethod
    @classmethod
    def p(cls, *a): return cls.__name__, a
print(P.p(1))
try: f.__get__(None, None)
except TypeError as e: print(e)

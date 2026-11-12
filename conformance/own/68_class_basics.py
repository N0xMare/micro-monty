# Class bodies, attributes, methods, bound methods, introspection.
class Counter:
    """Counts things."""
    total = 0
    kinds = []

    def __init__(self, start=0):
        self.n = start
        Counter.total += 1

    def inc(self, by=1):
        self.n += by
        return self

    def get(self):
        return self.n


c = Counter(5)
print(c.inc().inc(3).get(), Counter.total)
d = Counter()
print(Counter.total, c.total, d.n)
c.total = 100
print(c.total, Counter.total, d.total)
m = c.inc
print(m.__self__ is c, m.__func__ is Counter.inc, m.__func__.__qualname__)
print(m(10).n)
print(Counter.__name__, Counter.__qualname__, Counter.__module__, Counter.__doc__)
print(type(c) is Counter, c.__class__ is Counter, type(Counter) is type)
print(sorted(k for k in Counter.__dict__ if not k.startswith('__')))
print(c.__dict__, d.__dict__)
c.__dict__['extra'] = 1
print(c.extra, vars(c))
print(Counter.__bases__, Counter.__mro__)
print(Counter.inc.__name__, Counter.inc.__qualname__)


class Empty:
    pass


e = Empty()
e.a = 1
e.b = 2
del e.a
print(e.__dict__, hasattr(e, 'a'), getattr(e, 'a', 'dflt'))
setattr(e, 'z', 9)
print(e.z, Empty.__doc__)
try:
    e.missing
except AttributeError as err:
    print(err)
try:
    del e.a
except AttributeError as err:
    print(err)
try:
    Empty.nope
except AttributeError as err:
    print(err)


def outer():
    class Inner:
        def meth(self):
            return 1
    return Inner


I = outer()
print(I.__qualname__, I.meth.__qualname__, I().meth())
print(repr(I).startswith("<class '__main__.outer.<locals>.Inner'>"))
print(str(Counter), repr(int), str(type(None)))

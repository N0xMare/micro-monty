# __call__, bound/unbound methods, method identity and binding corners.
class Adder:
    def __init__(self, n):
        self.n = n

    def __call__(self, x, *rest, scale=1):
        return (self.n + x + sum(rest)) * scale


add5 = Adder(5)
print(add5(1), add5(1, 2, 3, scale=2), callable(add5), callable(Adder), callable(3))
print(list(map(add5, [1, 2])), sorted([3, 1, 2], key=Adder(0)))


class Obj:
    def method(self, a, b=2):
        return (self, a, b)

    def __eq__(self, o):
        return isinstance(o, Obj)

    __hash__ = None


o = Obj()
m1, m2 = o.method, o.method
print(m1 == m2, m1 is m2, m1(1)[1:], Obj.method(o, 3, b=4)[1:])
try:
    Obj.method()
except TypeError as e:
    print(e)
try:
    o.method()
except TypeError as e:
    print(e)
try:
    o.method(1, 2, 3)
except TypeError as e:
    print(e)
try:
    o.method(1, c=3)
except TypeError as e:
    print(e)
try:
    Adder()
except TypeError as e:
    print(e)
try:
    Adder(1, 2)
except TypeError as e:
    print(e)


class NoInit:
    pass


try:
    NoInit(1)
except TypeError as e:
    print(e)


class InitReturns:
    def __init__(self):
        return 5


try:
    InitReturns()
except TypeError as e:
    print(e)


def free(self):
    return 'free function as method'


class Holder:
    f = free
    g = staticmethod(free)
    lam = lambda self, x: x * 2


h = Holder()
print(h.f(), h.g('explicit'), h.lam(21))
h.f = lambda: 'instance attr'
print(h.f(), Holder.f(h))


class Chain:
    def __init__(self):
        self.ops = []

    def add(self, x):
        self.ops.append(x)
        return self


print(Chain().add(1).add(2).ops)
meths = [Chain().add for _ in range(2)]
print(meths[0] == meths[1], type(meths[0]).__name__)
bound = [1, 2, 3].append
bound(4)
print(type(bound).__name__, bound.__self__, bound.__name__)


class CallCls:
    def __call__(self):
        return 'instance call'


print(CallCls()())
try:
    NoInit()()
except TypeError as e:
    print(e)


class CallableAttr:
    def __init__(self):
        self.fn = lambda: 'attr callable'


print(CallableAttr().fn())


class Rec:
    def __call__(self, n):
        return 1 if n == 0 else n * self(n - 1)


print(Rec()(10))
print(repr(o.method).startswith('<bound method Obj.method of'), repr(Obj.method).startswith('<function Obj.method at'))

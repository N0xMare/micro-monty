# Class-body scope rules with comprehensions, generators and lambdas.
x = 'module'


class Scope:
    x = 'class'
    items = [1, 2, 3]
    doubled = [i * 2 for i in items]
    pairs = [(i, j) for i in items for j in range(i)][:3]
    total = sum(i for i in items)
    lam = lambda self: x
    first = next(i for i in items)
    try:
        bad = [x for _ in range(1)]
    except NameError:
        bad = 'NameError'

    def method(self):
        return x, self.x, Scope.x

    def gen(self):
        for i in self.items:
            yield i + len(x)


s = Scope()
print(Scope.doubled, Scope.pairs, Scope.total, s.lam(), Scope.first, Scope.bad)
print(s.method(), list(s.gen()))


def make():
    y = 'enclosing'

    class Inner:
        z = y
        w = [y for _ in range(2)]

        def get(self):
            return y

    y = 'rebound'
    return Inner


I = make()
print(I.z, I.w, I().get())


class Counter:
    count = 0
    incs = [count + i for i in range(3)] if False else None

    def __init__(self):
        Counter.count += 1
        self.id = Counter.count


print([Counter().id for _ in range(3)], Counter.count)


class Gen:
    values = (v * v for v in range(4))


print(list(Gen.values), list(Gen.values))


def factory(n):
    class Made:
        size = n

        def __repr__(self):
            return f'Made({self.size})'
    return Made()


print([factory(i) for i in range(3)])


class Outer:
    class Nested:
        def who(self):
            return type(self).__qualname__

    def make(self):
        return self.Nested()


print(Outer().make().who(), Outer.Nested.__qualname__, repr(Outer.Nested).endswith("Outer.Nested'>"))

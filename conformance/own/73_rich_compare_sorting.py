# Rich comparisons: reflection, NotImplemented, sorting, min/max, total order.
class Ver:
    def __init__(self, *parts):
        self.parts = parts

    def __repr__(self):
        return 'Ver' + repr(self.parts)

    def __eq__(self, o):
        if not isinstance(o, Ver):
            return NotImplemented
        return self.parts == o.parts

    def __lt__(self, o):
        if not isinstance(o, Ver):
            return NotImplemented
        return self.parts < o.parts

    def __hash__(self):
        return hash(self.parts)


vs = [Ver(1, 2), Ver(1, 0), Ver(0, 9), Ver(1, 2, 1), Ver(1, 0)]
print(sorted(vs))
print(sorted(vs, reverse=True))
print(min(vs), max(vs), max(vs, key=lambda v: -v.parts[0]))
print(Ver(1) == Ver(1), Ver(1) != Ver(2), Ver(1) == 1, 1 == Ver(1), Ver(1) != 'x')
print(Ver(2) > Ver(1), Ver(1) > Ver(2))
vs.sort()
print(vs, vs.index(Ver(1, 2)), vs.count(Ver(1, 0)), Ver(0, 9) in vs, Ver(5) in vs)
vs.remove(Ver(1, 0))
print(vs)
try:
    Ver(1) <= Ver(2)
except TypeError as e:
    print(e)
try:
    Ver(1) < 3
except TypeError as e:
    print(e)
try:
    sorted([Ver(1), 2])
except TypeError as e:
    print(e)


class Stable:
    def __init__(self, k, tag):
        self.k, self.tag = k, tag

    def __lt__(self, o):
        return self.k < o.k

    def __repr__(self):
        return f'{self.k}{self.tag}'


items = [Stable(2, 'a'), Stable(1, 'b'), Stable(2, 'c'), Stable(1, 'd'), Stable(0, 'e')]
print(sorted(items), sorted(items, reverse=True))
print(sorted(items, key=lambda s: s.tag, reverse=True))
print(sorted([3, 1, 2], key=lambda x: Stable(-x, '')))


class Spy:
    calls = []

    def __init__(self, n):
        self.n = n

    def __lt__(self, o):
        Spy.calls.append(('lt', self.n, o.n))
        return self.n < o.n

    def __gt__(self, o):
        Spy.calls.append(('gt', self.n, o.n))
        return self.n > o.n


print(Spy(1) > Spy(2), Spy.calls)
Spy.calls.clear()
print(Spy(1) < 5 if False else 'skip')


class OnlyGt:
    def __init__(self, n):
        self.n = n

    def __gt__(self, o):
        return self.n > o.n

    def __repr__(self):
        return f'G{self.n}'


print(sorted([OnlyGt(3), OnlyGt(1), OnlyGt(2)]))
print(min(OnlyGt(3), OnlyGt(1)), max([OnlyGt(3), OnlyGt(5)]))
print([1, Ver(1)] == [1, Ver(1)], (Ver(1),) < (Ver(2),), [Ver(1)] != [Ver(1)])


class Weird:
    def __eq__(self, o):
        return 'yes'

    def __ne__(self, o):
        return 0


w = Weird()
print(w == 1, w != 1, 1 == w, [w] == [1], w in [1])

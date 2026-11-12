# Subclasses of builtin types (M7): the value is extended, methods and
# operators fall back to it, and overrides win.


class Stack(list):
    def push(self, x):
        self.append(x)
        return self

    def __repr__(self):
        return 'Stack(' + super().__repr__() + ')'


s = Stack([1, 2])
s.push(3).push(4)
print(s, len(s), s[0], s[-1], s[1:3], 3 in s, isinstance(s, list), type(s).__name__)
print(sorted(s, reverse=True), sum(s), list(reversed(s)), s == [1, 2, 3, 4])
s += [5]
print(s, type(s).__name__)


class Registry(dict):
    def __missing__(self, key):
        return key.upper()


r = Registry(a=1)
print(r['a'], r['zz'], 'zz' in r, sorted(r.items()), len(r))


class Meters(float):
    def __str__(self):
        return f'{float(self)} m'


m = Meters(2.5)
print(str(m), m + 1, m * 2, round(m), isinstance(m, float))


class Name(str):
    def shout(self):
        return self.upper() + '!'


n = Name('ada')
print(n.shout(), n.title(), n + 'x', len(n), n == 'ada', {n: 1}['ada'])


class Pair(tuple):
    def __new__(cls, a, b):
        return super().__new__(cls, (a, b))

    def swap(self):
        return Pair(self[1], self[0])


p = Pair(1, 2)
print(p, p.swap(), p + (3,), hash(p) == hash((1, 2)), type(p.swap()).__name__)
a, b = p
print(a, b)


class Count(int):
    pass


c = Count(5)
print(c + 1, c * c, -c, c == 5, type(c + 1).__name__, f'{c:03d}', range(c)[-1], int(c), float(Meters(1)))


class Tally(dict):
    def __setitem__(self, k, v):
        super().__setitem__(k, self.get(k, 0) + v)

    def __repr__(self):
        return 'Tally' + super().__repr__()


t = Tally()
t['a'] = 2
t['a'] = 3
print(t, dict.__len__(t), list.__getitem__([1, 2], 1), super(Stack, s).__len__())

try:
    class Bad(bool):
        pass
except TypeError as e:
    print('TypeError', e)


class Shown:
    def __repr__(self):
        return 'Shown!'


class Bag(dict):
    pass


class Row(list):
    def __repr__(self):
        return 'Row' + super().__repr__()


bag = Bag(a=Shown())
bag['self'] = bag
print(bag, Row([Shown(), 1]), [Bag(x=Shown())], str(Row([2])))

# __len__, __bool__, __iter__/__next__, __contains__, __getitem__/__setitem__/__delitem__.
class Bag:
    def __init__(self, *items):
        self.items = list(items)

    def __len__(self):
        return len(self.items)

    def __iter__(self):
        return iter(self.items)

    def __contains__(self, x):
        print('contains', x)
        return x in self.items

    def __getitem__(self, i):
        if isinstance(i, slice):
            return Bag(*self.items[i])
        return self.items[i]

    def __setitem__(self, i, v):
        self.items[i] = v

    def __delitem__(self, i):
        del self.items[i]

    def __repr__(self):
        return f'Bag{self.items}'


b = Bag(1, 2, 3)
print(len(b), bool(b), bool(Bag()), list(b), 2 in b, 5 not in b)
print(b[0], b[-1], b[1:], b[::2])
b[0] = 'x'
del b[1]
print(b, sorted(Bag(3, 1, 2)), max(Bag(4, 9)), sum(Bag(1, 2)))
x, *rest = Bag(1, 2, 3)
print(x, rest, [*Bag(7, 8)], {*Bag(1, 1)}, dict.fromkeys(Bag('a')) if False else 'ok')
if Bag():
    print('truthy')
else:
    print('empty bag is falsy')
print('yes' if Bag(0) else 'no', not Bag(), Bag() or 'fallback', Bag(1) and 'second')


class OldSeq:
    def __getitem__(self, i):
        if i >= 3:
            raise IndexError(i)
        return i * 10


print(list(OldSeq()), 20 in OldSeq(), 25 in OldSeq(), list(enumerate(OldSeq())))
for v in OldSeq():
    print('seq', v)


class Countdown:
    def __init__(self, n):
        self.n = n

    def __iter__(self):
        return self

    def __next__(self):
        if self.n <= 0:
            raise StopIteration
        self.n -= 1
        return self.n


print(list(Countdown(3)), [x * 2 for x in Countdown(2)], tuple(Countdown(2)))
print(next(Countdown(5)), next(iter(Countdown(0)), 'empty'))


class Truthy:
    def __init__(self, v):
        self.v = v

    def __bool__(self):
        print('bool called')
        return self.v


print(bool(Truthy(True)), not Truthy(False), [1 for t in [Truthy(True)] if t])
try:
    bool(type('B', (), {'__bool__': lambda s: 1})())
except TypeError as e:
    print(e)


class BadLen:
    def __len__(self):
        return -1


try:
    len(BadLen())
except ValueError as e:
    print(e)
try:
    bool(BadLen())
except ValueError as e:
    print(e)


class StrLen:
    def __len__(self):
        return '3'


try:
    len(StrLen())
except TypeError as e:
    print(e)


class NotIter:
    pass


for bad in (lambda: iter(NotIter()), lambda: len(NotIter()), lambda: NotIter()[0], lambda: 1 in NotIter()):
    try:
        bad()
    except TypeError as e:
        print(e)
try:
    del NotIter()[0]
except TypeError as e:
    print(e)
try:
    NotIter()[0] = 1
except TypeError as e:
    print(e)


class BadIter:
    def __iter__(self):
        return 5


try:
    iter(BadIter())
except TypeError as e:
    print(e)


class Grid:
    def __getitem__(self, key):
        return key


g = Grid()
print(g[1, 2], g[1:2], g[::], g['k'])

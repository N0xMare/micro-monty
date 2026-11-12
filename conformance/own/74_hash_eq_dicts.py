# User __hash__/__eq__ in dicts and sets; eq without hash is unhashable.
class Key:
    probes = 0

    def __init__(self, name, h):
        self.name, self.h = name, h

    def __hash__(self):
        return self.h

    def __eq__(self, o):
        Key.probes += 1
        return isinstance(o, Key) and self.name == o.name

    def __repr__(self):
        return f'Key({self.name!r})'


a, b, c = Key('a', 1), Key('b', 1), Key('c', 2)
d = {a: 1, b: 2, c: 3}
print(d, len(d))
print(d[Key('b', 1)], Key('a', 1) in d, Key('z', 1) in d, d.get(Key('q', 7), 'none'))
d[Key('a', 1)] = 10
print(d)
del d[Key('b', 1)]
print(d, d.pop(Key('c', 2)), d)
print(Key.probes > 0)
s = {a, b, c, Key('a', 1)}
print(len(s), Key('c', 2) in s)
s.discard(Key('a', 1))
s.add(Key('d', 3))
print(sorted(k.name for k in s))
print({Key('x', 5): 1} == {Key('x', 5): 1}, {Key('x', 5)} == {Key('x', 5)})
d2 = dict.fromkeys([1]) if False else {}
for i in range(20):
    d2[Key(str(i), i % 3)] = i
print(len(d2), d2[Key('17', 2)], list(d2.values())[:5])
print(dict([(Key('p', 0), 1), (Key('p', 0), 2)]))
print(set([Key('p', 0), Key('p', 0), Key('q', 0)]))


class EqOnly:
    def __eq__(self, o):
        return True


print(EqOnly.__hash__)
try:
    {EqOnly(): 1}
except TypeError as e:
    print(e)
try:
    hash(EqOnly())
except TypeError as e:
    print(e)
try:
    {EqOnly()}
except TypeError as e:
    print(e)


class Plain:
    pass


p = Plain()
print(hash(p) == hash(p), {p: 1}[p], p == p, p != Plain())


class InheritsHash(Key):
    pass


print({InheritsHash('i', 9): 1})


class BadHash:
    def __hash__(self):
        return 'nope'


try:
    hash(BadHash())
except TypeError as e:
    print(e)


class BigHash:
    def __hash__(self):
        return 2 ** 62 + 5


print(hash(BigHash()) == hash(2 ** 62 + 5))


class Restore(EqOnly):
    __hash__ = object.__hash__


r = Restore()
print({r: 'ok'}[r])
print(hash((1, 'a')) == hash((1, 'a')), hash((Key('t', 4), 1)) == hash((Key('t', 4), 1)))

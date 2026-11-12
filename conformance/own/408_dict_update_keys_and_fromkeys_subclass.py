# `dict.update`/`dict()` on a user object call its `keys()` first, so an
# error raised there propagates (it was a TypeError "not iterable"), and
# `fromkeys` on a dict subclass builds an instance of the subclass through
# its `__setitem__`.
class Boom(Exception):
    pass

class BadKeys:
    def keys(self):
        raise Boom('from keys')

for make in (lambda s: {}.update(s), dict):
    try:
        make(BadKeys())
    except Boom as e:
        print('Boom', e)

class Mapping:
    def keys(self):
        return ['a', 'b']
    def __getitem__(self, k):
        return k.upper()

d = {'z': 0}
d.update(Mapping(), c=3)
print(d, dict(Mapping()))

class NotIterable:
    pass

try:
    dict(NotIterable())
except TypeError as e:
    print(e)

class Logged(dict):
    def __setitem__(self, k, v):
        print('set', k, v)
        super().__setitem__(k, v)

x = Logged.fromkeys('ab', 0)
print(type(x).__name__, x)
y = Logged().fromkeys(range(2))
print(type(y).__name__, y)
print(type(dict.fromkeys('a')).__name__)

from collections import OrderedDict, Counter, defaultdict
print(OrderedDict.fromkeys('xy', 1), type(defaultdict.fromkeys('q')).__name__)
print({}.fromkeys(iter([5])), Logged().fromkeys('z', 1), type(OrderedDict().fromkeys('ab')).__name__)

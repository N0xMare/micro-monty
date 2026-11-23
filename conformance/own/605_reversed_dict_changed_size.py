# `reversed()` of a dict and of its views is a live iterator over the dict:
# a change of size while it runs raises RuntimeError (sticky, as CPython's),
# a change that keeps the size goes on, and the type names are CPython's.
from collections.abc import Iterator


def t(label, mk, mut):
    d = {i: i * 10 for i in range(4)}
    out = []
    try:
        for x in mk(d):
            out.append(x)
            mut(d, x)
        print(label, 'ok', out, d)
    except RuntimeError as e:
        print(label, 'RuntimeError', e, out, d)


def nop(d, x):
    pass


def pop_first(d, x):
    d.pop(next(iter(d)))


def add(d, x):
    d[len(d) + 100] = 0


def replace_last(d, x):
    if 50 not in d:
        d.pop(3)
        d[50] = 5


def set_value(d, x):
    d[0] = 'v'


for label, mk in [
    ('dict', reversed),
    ('keys', lambda d: reversed(d.keys())),
    ('values', lambda d: reversed(d.values())),
    ('items', lambda d: reversed(d.items())),
    ('slot', lambda d: d.__reversed__()),
]:
    for mut in (nop, pop_first, add, replace_last, set_value):
        t(label + ' ' + mut.__name__, mk, mut)

d = {1: 1, 2: 2}
r = reversed(d)
print(type(r).__name__, type(reversed(d.values())).__name__, type(reversed(d.items())).__name__)
print(isinstance(r, Iterator), iter(r) is r, list(r), list(r))
r = reversed(d)
d[3] = 3
for _ in range(2):
    try:
        next(r)
    except RuntimeError as e:
        print('again', e)
d.pop(3)
try:
    print(next(r))
except RuntimeError as e:
    print('sticky', e)
print(list(reversed({})))
d = {i: i for i in range(200)}
for i in range(1, 199):
    del d[i]
print(list(reversed(d)), list(reversed(d.items())))
r = reversed(d)
print(next(r))
del d[199]
d[7] = 7
print(list(r))
print(2 in reversed({1: 1, 2: 2}), 5 in reversed({1: 1}))


class D(dict):
    pass


print(list(reversed(D(a=1, b=2))), next(reversed({'a': 1}.items())))

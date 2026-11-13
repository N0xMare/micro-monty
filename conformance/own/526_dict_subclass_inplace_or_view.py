# N1 (correctness review, landing 8): a dict subclass's instance `|=` a
# dict view updates the instance in place, as dict's `|=`, for every
# left-hand type; a keys view of non-pairs is refused as an update
# sequence ("object is not iterable", CPython 3.14's text).
from collections import OrderedDict, Counter, defaultdict


class DD(dict):
    pass


def t(name, f):
    try:
        r = f()
    except Exception as e:
        r = f'{type(e).__name__}: {e}'
    print(name, r)


def run(make, rhs, label):
    x = make()
    y = x
    try:
        x |= rhs()
        print(label, type(x).__name__, x is y, sorted(x.items(), key=repr), sorted(y.items(), key=repr))
    except Exception as e:
        print(label, f'{type(e).__name__}: {e}', x is y, sorted(y.items(), key=repr))


makers = [
    ('dict', lambda: {1: 0}),
    ('DD', lambda: DD({1: 0})),
    ('OrderedDict', lambda: OrderedDict({1: 0})),
    ('Counter', lambda: Counter({1: 0})),
    ('defaultdict', lambda: defaultdict(int, {1: 0})),
]
rhss = [
    ('items', lambda: {2: 3}.items()),
    ('keys', lambda: {2: 3}.keys()),
    ('pair-keys', lambda: {(2, 3): 0}.keys()),
    ('values', lambda: {2: (4, 5)}.values()),
    ('DD items', lambda: DD({2: 3}).items()),
]
for mn, mk in makers:
    for rn, rh in rhss:
        # OrderedDict's own update unpacks pairs with other messages.
        if (mn, rn) != ('OrderedDict', 'keys'):
            run(mk, rh, f'{mn} |= {rn}:')
for mn, mk in makers:
    t(f'{mn} | items:', lambda mk=mk: sorted(mk() | {2: 3}.items(), key=repr))
    t(f'items | {mn}:', lambda mk=mk: sorted({2: 3}.items() | mk(), key=repr))
    t(f'keys - {mn}:', lambda mk=mk: {1: 0, 2: 0}.keys() - mk())
for f in (lambda: {}.update([2]), lambda: dict([2]), lambda: dict({2: 3}.keys()),
          lambda: {}.update(iter([2])), lambda: {}.update(x for x in [2]), lambda: {}.update([[1]])):
    t('update non-pair:', f)

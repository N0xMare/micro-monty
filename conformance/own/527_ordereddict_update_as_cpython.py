# OrderedDict's update, `__init__` and `|=` are CPython's
# `mutablemapping_update`: a subclass's `__setitem__` stores each key, a
# mapping is read through `keys()` and `[]`, and pairs unpack with
# `mutablemapping_add_pairs`'s messages.
from collections import OrderedDict


class O(OrderedDict):
    def __setitem__(self, k, v):
        print('set', k, v)
        OrderedDict.__setitem__(self, k, v)


class DS(dict):
    def __getitem__(self, k):
        print('getitem', k)
        return dict.__getitem__(self, k) * 10


class M:
    def keys(self):
        return ['a', 'b']

    def __getitem__(self, k):
        return k * 2


def t(name, f):
    try:
        print(name, f())
    except Exception as e:
        print(name, type(e).__name__, e)


def upd(d, *a, **k):
    d.update(*a, **k)
    return list(d.items())


for src in ([2], [[1]], [[]], [[1, 2, 3]], ['ab'], [(1, 2)], {2: 3}.keys(),
            {(1, 2): 0}.keys(), {1: 2}, [[1, 2], [3]], [iter([5, 6])]):
    t('init', lambda: list(OrderedDict(src).items()))
    t('update', lambda: upd(OrderedDict({0: 0}), src))
t('init DS', lambda: list(OrderedDict(DS({1: 2})).items()))
t('init M', lambda: list(OrderedDict(M()).items()))
t('update kw', lambda: upd(OrderedDict(), [(1, 2)], a=3))
t('update self', lambda: (lambda d: upd(d, d))(OrderedDict({1: 2, 3: 4})))
t('O init', lambda: list(O([(1, 2)], z=0).items()))
t('O init dict', lambda: list(O({3: 4}).items()))
t('O update kw', lambda: upd(O(), a=1))
t('O ior', lambda: (lambda o: (o.__ior__({5: 6}), list(o.items()))[1])(O()))
t('O or', lambda: list((O() | {7: 8}).items()))
t('O fromkeys', lambda: list(O.fromkeys('ab', 1).items()))
t('O copy', lambda: list(O({9: 9}).copy().items()))
t('order', lambda: list(OrderedDict([(3, 0), (1, 0), (3, 1)], b=2, a=1).items()))

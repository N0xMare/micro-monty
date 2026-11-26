# A dict subclass that defines `__iter__` merges through `keys()` and
# `__getitem__` in `|`, `|=` and `copy()`, as CPython's `dict_merge` (and
# as `dict(o)`, `{**o}` and `update(o)` already did); without its own
# `__iter__` the stored items are copied, whatever `keys()` or
# `__getitem__` say. The results are plain dicts.


class It(dict):
    def __iter__(self):
        return iter(['a'])

    def __getitem__(self, k):
        return 'item'


class K(dict):
    def keys(self):
        return ['b']

    def __getitem__(self, k):
        return 'kk'


o = It({'a': 1, 'b': 2})
merged = {'a': 'item', 'b': 'item'}
assert {'z': 0} | o == {'z': 0, **merged} and o | {'z': 0} == {**merged, 'z': 0}
assert o.copy() == merged and dict.copy(o) == merged and type(o.copy()) is dict
assert o | o == merged and type(o | {}) is dict
d = {'z': 0}
d |= o
assert d == {'z': 0, **merged}
p = It({'q': 5})
p |= {'z': 0}
assert type(p) is It and list(dict.items(p)) == [('q', 5), ('z', 0)]
p |= It({'r': 1})
assert type(p) is It and list(dict.items(p)) == [('q', 5), ('z', 0), ('r', 'item')]
assert dict(o) == {**o} == merged

k = K({'a': 1, 'b': 2})
assert k.copy() == {'a': 1, 'b': 2} and {} | k == {'a': 1, 'b': 2}
assert list((K({'a': 1, 'z': 3}) | o).items()) == [('a', 'item'), ('z', 3), ('b', 'item')]
try:
    dict.copy([])
except TypeError as e:
    assert str(e) == "descriptor 'copy' for 'dict' objects doesn't apply to a 'list' object", e

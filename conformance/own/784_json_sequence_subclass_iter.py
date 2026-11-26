# `json.dumps` of a list or tuple subclass that defines `__iter__` encodes
# what it yields, as CPython does, nested or not and in every output form,
# empty or not by what it yields, whatever its `__bool__` or `__len__`
# say; a subclass that overrides only `__len__` or `__getitem__` encodes
# its items.
import json


class L(list):
    def __iter__(self):
        return iter([7, 8])


class T(tuple):
    def __iter__(self):
        return iter([7, 8])


class F(list):
    def __iter__(self):
        return iter([7, 8])

    def __bool__(self):
        return False


class Empty(tuple):
    def __iter__(self):
        return iter([])

    def __len__(self):
        return 5


class Len(list):
    def __len__(self):
        return 0


class Item(list):
    def __getitem__(self, i):
        return 9


for o in (L([1, 2, 3]), T((1, 2, 3))):
    assert json.dumps(o) == '[7, 8]'
    assert json.dumps(o, indent=1) == '[\n 7,\n 8\n]'
    assert json.dumps(o, separators=(',', ':')) == '[7,8]'
assert json.dumps({'k': L([1])}, sort_keys=True) == '{"k": [7, 8]}'
assert json.dumps([T((1,))]) == '[[7, 8]]'
assert json.dumps(Len([1, 2])) == '[1, 2]' and json.dumps(Item([1, 2])) == '[1, 2]'
for kw in ({}, {'indent': 1}):
    assert json.dumps(F([1]), **kw) == json.dumps([7, 8], **kw)
    assert json.dumps(Empty((1,)), **kw) == '[]' and json.dumps([Empty()], **kw) == json.dumps([[]], **kw)

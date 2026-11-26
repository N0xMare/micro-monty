# `json.dumps` of a dict subclass judges it empty by its stored entries,
# as CPython does, whatever its `__len__` or `__bool__` say, and then
# encodes what its own `items()` gives.
import json


class D(dict):
    def items(self):
        return [('a', 1)]

    def __len__(self):
        return 0

    def __bool__(self):
        return False


class Z(dict):
    def items(self):
        return []


for kw in ({}, {'indent': 1}, {'sort_keys': True}):
    assert json.dumps(D(q=1), **kw) == json.dumps({'a': 1}, **kw)
    assert json.dumps([D(q=2)], **kw) == json.dumps([{'a': 1}], **kw)
    assert json.dumps(D(), **kw) == '{}' and json.dumps(Z(q=1), **kw) == '{}'

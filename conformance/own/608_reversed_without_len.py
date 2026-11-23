# `reversed(o)` of an object with `__getitem__` but no `__len__` or
# `__reversed__` raises CPython's TypeError that it has no length; an object
# with neither is not reversible; one with both reverses through them.
def t(label, f):
    try:
        print(label, list(f()))
    except Exception as e:
        print(label, type(e).__name__, e)


class GetItem:
    def __getitem__(self, i):
        return i


class LenOnly:
    def __len__(self):
        return 2


class Neither:
    pass


class Both:
    def __getitem__(self, i):
        return i * 10

    def __len__(self):
        return 3


class Inherits(GetItem):
    pass


class ListItem(list):
    def __getitem__(self, i):
        return 'item'


class DictItem(dict):
    def __getitem__(self, k):
        return 'item'


t('getitem only', lambda: reversed(GetItem()))
t('inherited getitem', lambda: reversed(Inherits()))
t('len only', lambda: reversed(LenOnly()))
t('neither', lambda: reversed(Neither()))
t('both', lambda: reversed(Both()))
t('list subclass', lambda: reversed(ListItem([1, 2])))
t('dict subclass', lambda: reversed(DictItem(a=1, b=2)))
t('int', lambda: reversed(3))
t('generator', lambda: reversed(x for x in []))

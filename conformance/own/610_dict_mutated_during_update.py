# A merge whose key's `__eq__` empties the source dict raises CPython's
# `RuntimeError: dict mutated during update` after the insert that ran
# it (`dict_merge` checks the source's size after each insert); an items
# view raises its iterator's error. A target that is empty at the start
# is not here: CPython copies a compact source without comparing keys.


class K:
    def __init__(self, name, victim=None):
        self.name = name
        self.victim = victim

    def __hash__(self):
        return 1

    def __eq__(self, other):
        # Whichever side compares, the key that holds the source empties
        # it once.
        for k in (self, other):
            if isinstance(k, K) and k.victim is not None:
                k.victim.clear()
                k.victim = None
        return False

    def __repr__(self):
        return self.name


def fresh():
    src = {}
    src[K('a', src)] = 1
    src['x'] = 2
    return src


def show(what, f):
    src = fresh()
    try:
        r = f(src)
    except RuntimeError as e:
        print(what, 'RuntimeError', e, 'src', src)
    else:
        print(what, 'ok', r, 'src', src)


def target():
    return {K('b'): 0}


def update(src):
    d = target()
    d.update(src)
    return d


def ior(src):
    d = target()
    d |= src
    return d


def update_kw(src):
    d = target()
    d.update(src, y=3)
    return d


def items(src):
    d = target()
    d.update(src.items())
    return d


def ior_items(src):
    d = target()
    d |= src.items()
    return d


class Sub(dict):
    pass


def sub_ior(src):
    d = Sub(target())
    d |= src
    return d


import collections


def od_update(src):
    d = collections.OrderedDict(target())
    d.update(src)
    return list(d.items())

cases = [
    ('update', update),
    ('|=', ior),
    ('{**}', lambda s: {**target(), **s}),
    ('|', lambda s: target() | s),
    ('update kw', update_kw),
    ('items', items),
    ('|= items', ior_items),
    ('dict.update', lambda s: dict.update(target(), s)),
    ('map', lambda s: list(map(target().update, [s]))),
    ('Sub source', lambda s: target().update(Sub(s))),
    ('Sub target', lambda s: Sub(target()).update(s)),
    ('Sub |', lambda s: Sub(target()) | s),
    ('Sub |=', sub_ior),
    ('defaultdict', lambda s: collections.defaultdict(int, target()).update(s)),
    ('Counter', lambda s: collections.Counter(target()).update(s)),
    # An OrderedDict reads an exact dict as a list of its items.
    ('OrderedDict', od_update),
]
for what, f in cases:
    show(what, f)

# The source emptied by a key of the target: the same error, and the
# insert is done.
src = {K('a'): 1, 'x': 2}
d = {K('b', src): 0}
try:
    d.update(src)
except RuntimeError as e:
    print('target key', e, d, src)


# A key whose `__eq__` changes the source but not its size raises
# nothing: the walk goes on over the source's entries.
class Swap(K):
    __hash__ = K.__hash__

    def __eq__(self, other):
        for k in (self, other):
            if isinstance(k, Swap) and k.victim is not None:
                v, k.victim = k.victim, None
                del v['x']
                v['z'] = 9
        return False


src = {}
src[Swap('a', src)] = 1
src['x'] = 2
d = {K('b'): 0}
d.update(src)
print('same size', d, src)

# A source that grows raises as well.
class Grow(K):
    __hash__ = K.__hash__

    def __eq__(self, other):
        for k in (self, other):
            if isinstance(k, Grow) and k.victim is not None:
                k.victim['new'] = 0
                k.victim = None
        return False


src = {}
src[Grow('a', src)] = 1
src['x'] = 2
d = {K('b'): 0}
try:
    d.update(src)
except RuntimeError as e:
    print('grow', e, d, src)

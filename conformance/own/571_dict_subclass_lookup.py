# A dict subclass's `x in d` without its own `__contains__` and its
# `super().__getitem__(k)` take dict's own lookup, as CPython's
# `dict_contains` and `dict_subscript` do, with keys only Python can
# compare: `in` compares only the keys on its probe (it scanned every key),
# an `__eq__` that stores during it does not raise, and a `__getitem__`
# that delegates to `super()` does not re-enter itself.
import collections


class K:
    st = None

    def __init__(self, v): self.v = v

    def __hash__(self): return hash(self.v) & 7

    def __eq__(self, o):
        global n
        n += 1
        a, K.st = K.st, None
        if a: a()
        return isinstance(o, K) and self.v == o.v


n = 0


class Lie(dict):
    def __contains__(self, k): return False


class Sup(collections.OrderedDict):
    def __contains__(self, k): return not super().__contains__(k)


class OG(collections.OrderedDict):
    def __getitem__(self, k): return super().__getitem__(k)


class DG(dict):
    def __getitem__(self, k): return super().__getitem__(k)


class DM(dict):
    def __getitem__(self, k): return super().__getitem__(k)

    def __missing__(self, k): return 'missing'


kinds = (dict, collections.OrderedDict, collections.defaultdict, collections.Counter,
         Lie, Sup, OG, DG, DM)
for cls in kinds:
    d = cls()
    for i in range(20): d[K(i)] = i
    n = 0; r1 = K(5) in d; c1 = n
    n = 0; r2 = K(99) in d; c2 = n
    x = K(1000)
    def hook(): dict.__setitem__(d, x, 'in')
    K.st = hook
    try: r3 = x in d
    except Exception as e: r3 = type(e).__name__
    K.st = None
    out = []
    for what in ('get', 'miss'):
        n = 0
        try:
            if what == 'get': r = d[K(5)]
            else: r = d[K(3000)]
        except Exception as e: r = type(e).__name__
        out.append((what, r, n))
    print(cls.__name__, r1, c1, r2, c2, r3, len(d), out)

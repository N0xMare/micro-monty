# `setdefault` hashes and looks its key up once, then inserts, as CPython's
# `dict_setdefault_ref_lock_held` does: when an `__eq__` stores the key
# during that lookup, the stored value stays (CPython ends with the key in
# two entries, as its plain store does), and a value already held by an
# equal key is never overwritten with the default. `OrderedDict.setdefault`
# is CPython's own (`OrderedDict_setdefault_impl`): a lookup, then a store
# that looks the key up again, so the key stored meanwhile takes the
# default and is held once; a subclass goes through its own methods.
import collections

log = []


class K:
    act = None
    def __init__(self, v): self.v = v
    def __hash__(self):
        log.append('h')
        return 3

    def __eq__(self, o):
        log.append(self.v)
        a, K.act = K.act, None
        if a: a()
        return isinstance(o, K) and self.v == o.v


class OD(collections.OrderedDict):
    def __contains__(self, k):
        log.append('c')
        return super().__contains__(k)

    def __setitem__(self, k, v):
        log.append('s')
        super().__setitem__(k, v)


def run(n, ndel, how, kind=dict):
    ks = [K(i) for i in range(n + ndel)]
    d = kind()
    for k in ks: d[k] = 1
    for j in range(ndel):
        d.pop(ks[(j * 3 + 1) % len(ks)], None)
    x = K(1000)
    def hook(): d[x] = 'inner'
    K.act = hook
    del log[:]
    if how == 'setdefault': r = d.setdefault(x, 'outer')
    elif how == 'store': d[x] = 'outer'; r = None
    else: r = d.setdefault(K(0) if n else x, 'outer')
    K.act = None
    calls = (len(log), log.count('h')) if kind is dict else (log.count('c'), log.count('s'))
    print(kind.__name__, n, ndel, how, 'r', r, 'len', len(d), 'x', sum(1 for k in d if k is x),
          'vals', [v for k, v in d.items() if k is x], 'calls', *calls)


# An OrderedDict's own rows give no call totals (CPython's node table
# hashes and compares its keys again as it grows) and no store (a store
# that an `__eq__` overtakes is held once by CPython's OrderedDict, twice by
# mm's, which is older); a subclass's rows count its `__contains__` and
# `__setitem__` calls.
for kind in (dict, collections.OrderedDict, OD):
    for shape in ((33, 31), (40, 21), (40, 0), (20, 42), (12, 0)):
        for how in ('store', 'setdefault', 'present'):
            if kind is dict or how != 'store':
                run(shape[0], shape[1], how, kind)


# A subclass's `__contains__` is bound as `in` binds it (a staticmethod,
# classmethod or callable object takes the key alone), and a
# `__getitem__` that delegates to dict's reaches dict's lookup, not itself.
class CStatic(collections.OrderedDict):
    @staticmethod
    def __contains__(k): return False
class CClass(collections.OrderedDict):
    @classmethod
    def __contains__(cls, k): return k == 'a'
class Probe:
    def __call__(self, *a): return a == ('a',)
class CCall(collections.OrderedDict):
    __contains__ = Probe()
class OG(collections.OrderedDict):
    def __getitem__(self, k): return super().__getitem__(k)
for kind in (CStatic, CClass, CCall):
    o = kind(a=1)
    print(kind.__name__, o.setdefault('b', 2), o.setdefault('a', 3), list(o.items()))
g = OG()
for k in [K(i) for i in range(40)]: g[k] = 'v'
print('OG', g.setdefault(K(7), 'dflt'), g.setdefault(K(99), 'dflt'), len(g))

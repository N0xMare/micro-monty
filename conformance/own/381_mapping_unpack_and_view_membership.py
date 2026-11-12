# `{**m}` and `f(**m)` with mappings that are not dicts, and membership in
# dict views, which hashes the item as dict lookup does.

class M:
    def __init__(self, d): self.d = d
    def keys(self): return list(self.d)
    def __getitem__(self, k): return self.d[k]
def f(**kw): return kw
def g(a, b=0): return (a, b)
print({**M({'a': 1}), 'b': 2})
print({'z': 0, **M({'a': 1, 'b': 2})})
print(f(**M({'x': 1, 'y': 2})))
print(g(**M({'a': 3})), g(1, **M({'b': 4})))
class D(dict):
    def keys(self): return ['k']
    def __getitem__(self, k): return 'over'
print({**D(k=1)})
for bad in [lambda: {**5}, lambda: f(**5), lambda: f(**M({1: 2})), lambda: f(a=1, **M({'a': 2})), lambda: {**[1]}]:
    try: print(bad())
    except TypeError as e: print('TypeError', e)
import collections, types
print({**collections.ChainMap({'a': 1}, {'b': 2})}, {**types.MappingProxyType({'p': 1})})

class H0:
    def __hash__(self): return 1
    def __eq__(self, o): return True
d = {(1, 2): 0}
print((1, H0()) in d.keys(), (1, H0()) in d, H0() in {2: 0}.keys(), H0() in {1: 0}.keys())
print(((1, 2), H0()) in d.items(), ((1, H0()), 0) in d.items())
print({1: H0()}.items() == {(1, 2): 0}.keys())
print(d.keys() & {(1, H0())}, d.keys() - {(1, H0())})

class H:
    n = 0
    def __hash__(self):
        H.n += 1
        return 1
    def __eq__(self, o): return True
d = {(1, 2): 0, 1: 'one'}
print(H() in d.keys(), H.n, ((H(), 'one') in d.items()), H.n)
for bad in [lambda: [1] in d.keys(), lambda: ([1], 0) in d.items()]:
    try: print(bad())
    except TypeError as e: print('TypeError', e)
print((1, 2, 3) in d.items(), 5 in d.items())
class DS(dict): pass
print(H() in DS({1: 0}).keys(), (1, H()) in DS({(1, 2): 0}).keys())

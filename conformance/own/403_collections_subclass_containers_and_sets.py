# collections and sets as CPython 3.14: a dict/list/str subclass that
# overrides __getitem__ keeps its own `in` and iteration (an OrderedDict LRU
# cache); ChainMap views and `dict | ChainMap`; UserDict popitem/clear/
# fromkeys/|; UserList comparisons, * and +=; UserString *, %, radd and
# comparisons; a set (or set subclass) looked up in a set finds the equal
# frozenset; set subclasses with set operators.
from collections import OrderedDict, ChainMap, UserDict, UserList, UserString
class SS(set): pass
for f in [lambda: SS([1]) | {2}, lambda: {2} | SS([1]), lambda: SS([1]) & SS([1]), lambda: SS([1]) - {1}]:
    try: r = f(); print(type(r).__name__, r)
    except Exception as ex: print(type(ex).__name__, ex)
try: print(set() in {frozenset()})
except Exception as ex: print(ex)
try:
    s = {frozenset([1])}; s.remove({1}); print(s)
except Exception as ex: print(ex)
cm = ChainMap({'a': 1}, {'b': 2}); print(list(cm.items()), cm.items())
try: print({'z': 0} | cm)
except Exception as ex: print(type(ex).__name__, ex)
u = UserList([1,2]); 
for f in [lambda: u*2, lambda: 2*u, lambda: u > UserList([0]), lambda: [0] + u]:
    try: print(f())
    except Exception as ex: print(type(ex).__name__, ex)
us = UserString("ab")
for f in [lambda: us*2, lambda: "x"+us, lambda: us > "a", lambda: "%s" % us, lambda: us % ()]:
    try: print(repr(f()))
    except Exception as ex: print(type(ex).__name__, ex)
ud = UserDict(a=1)
for f in [lambda: ud.popitem(), lambda: UserDict.fromkeys("ab"), lambda: ud | UserDict(b=2)]:
    try: print(f())
    except Exception as ex: print(type(ex).__name__, ex)
d = {"a": 1}
class D(dict):
    def __getitem__(self, k): return dict.__getitem__(self, k) * 10
d = D(a=1)
print("a" in d, "b" in d, d["a"])
class L(list):
    def __getitem__(self, i): return list.__getitem__(self, i)
print(3 in L([1, 3]), 5 in L([1]))
class S(str):
    def __getitem__(self, i): return "x"
print("b" in S("abc"))
print(list(d), sorted(d), [k for k in L([4, 5])], list(S("ab")), {**d}, dict(d), len(d))
class G(list):
    def __iter__(self): return iter([9])
print(1 in G([1]), 9 in G([1]), list(G([1])))
from collections import OrderedDict
class LRU:
    def __init__(self, size):
        super().__init__()
        self.size = size
    def __getitem__(self, item):
        value = super().__getitem__(item)
        self.move_to_end(item)
        return value
    def __setitem__(self, key, value):
        while key not in self and len(self) >= self.size:
            self.popitem(last=False)
        super().__setitem__(key, value)
        self.move_to_end(key)
class C(LRU, OrderedDict): pass
c = C(2)
c['t1'] = 1; c['t2'] = 2; c['t3'] = 3
print(list(c), c)
class Cache(OrderedDict):
    def __init__(self, cap):
        super().__init__(); self.cap = cap
    def get(self, k):
        if k not in self: return -1
        self.move_to_end(k); return self[k]
    def put(self, k, v):
        self[k] = v; self.move_to_end(k)
        if len(self) > self.cap: self.popitem(last=False)
c2 = Cache(2); c2.put(1, 1); c2.put(2, 2); c2.get(1); c2.put(3, 3); print(c2, c2.get(2))
od = OrderedDict(a=1)
print(od.pop("zz", "dflt"), od.setdefault("b", default=5) if False else 0)
s = {frozenset([1, 2]), frozenset()}
print({1, 2} in s, set() in s, {3} in s, {2, 1} not in s)
s.remove({1, 2}); s.discard(set()); print(s)
try: s.remove({9})
except KeyError as e: print("KeyError", e)
try: {1} in {1: 2}
except TypeError as e: print(e)
fs = frozenset([frozenset([1])])
print({1} in fs, [1] in [[1]])
try: [1] in {1}
except TypeError as e: print(e)
class SS(set): pass
s2 = {frozenset([1])}
print(SS([1]) in s2); s2.discard(SS([1])); print(s2)
class Bad:
    def __init__(self, v): self.v = v
    def __eq__(self, o): return isinstance(o, Bad) and self.v == o.v
    def __hash__(self): return self.v % 2
class SS(set): pass
a = SS(Bad(i) for i in range(4)); b = set(Bad(i) for i in range(2, 6))
for f in [lambda: a & b, lambda: b | a, lambda: a - b, lambda: a ^ b]:
    try: r = f(); print(type(r).__name__, sorted(x.v for x in r))
    except Exception as e: print(type(e).__name__, e)
a |= b; print(type(a).__name__, len(a))
class R:
    def __rmod__(self, o): return "rmod"
    def __radd__(self, o): return "radd"
    def __str__(self): return "R"
print("%s" % R(), "%s-%s" % (R(), 1), 5 + R() if False else "", [1] + R())
class R2(str):
    def __rmod__(self, o): return "rmod2"
print("%s" % R2("x"))

# Instance shapes and inline caches: attribute order, deletion, overflow,
# class mutation invalidating cached attributes and methods.
class P:
    def __init__(self, a, b, first_a=True):
        if first_a:
            self.a = a
            self.b = b
        else:
            self.b = b
            self.a = a

    def total(self):
        return self.a + self.b


ps = [P(i, i * 10, i % 2 == 0) for i in range(6)]
print([p.total() for p in ps], [list(vars(p)) for p in ps[:2]])
for p in ps:
    p.a += 1
print(sum(p.a for p in ps))
del ps[0].a
try:
    ps[0].total()
except AttributeError as e:
    print(e)
ps[0].a = 100
print(ps[0].total(), list(vars(ps[0])))


class Wide:
    pass


w = Wide()
for i in range(40):
    setattr(w, f'attr{i}', i)
print(sum(getattr(w, f'attr{i}') for i in range(40)), len(vars(w)))
for i in range(0, 40, 3):
    delattr(w, f'attr{i}')
print(len(vars(w)), w.attr1, w.attr38, hasattr(w, 'attr39'))
w.__dict__.clear()
print(vars(w), hasattr(w, 'attr1'))


class Greeter:
    def greet(self):
        return 'hello'


g = Greeter()
out = []
for i in range(6):
    out.append(g.greet())
    if i == 2:
        Greeter.greet = lambda self: 'patched'
    if i == 4:
        g.greet = lambda: 'instance'
print(out)


class Base:
    def who(self):
        return 'base'


class Child(Base):
    pass


c = Child()
seen = []
for i in range(5):
    seen.append(c.who())
    if i == 1:
        Base.who = lambda self: 'base2'
    if i == 3:
        Child.who = lambda self: 'child'
print(seen)


class Attr:
    x = 'class'


a = Attr()
vals = []
for i in range(4):
    vals.append(a.x)
    if i == 0:
        a.x = 'inst'
    if i == 1:
        del a.x
    if i == 2:
        Attr.x = 'class2'
print(vals)


class Slotish:
    def __init__(self):
        self.v = 0


objs = [Slotish() for _ in range(3)]
objs[1].extra = 'e'
for _ in range(3):
    for o in objs:
        o.v += 1
print([o.v for o in objs], [sorted(vars(o)) for o in objs])


class Swap:
    pass


s = Swap()
s.__dict__['k'] = 1
s.k += 1
print(s.k, s.__dict__)


class PropLater:
    def __init__(self):
        self.val = 1


pl = PropLater()
reads = []
for i in range(4):
    reads.append(pl.val)
    if i == 1:
        PropLater.val = property(lambda self: 'property wins')
print(reads)


# Global caches across deleting and re-adding names: a deleted global's
# load and store miss, a builtin shadowed and unshadowed is seen at once.
seen = []
for i in range(4):
    gone = i
    seen.append(gone)
    del gone
    try:
        seen.append(gone)
    except NameError as e:
        seen.append(str(e))
    if i % 2:
        len = lambda s: -1
    seen.append(len('ab'))
    if i % 2:
        del len
    gone = i * 10
    gone += 1
    seen.append(gone)
print(seen, gone)
for i in range(3):
    once = i
    del once
print(i)
print([(a % b, a // b) for a in (7, -7, 0, -1) for b in (3, -3, 1)])

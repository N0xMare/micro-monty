# Special methods are looked up on the type, not the instance.
class C:
    def __len__(self):
        return 1

    def __repr__(self):
        return 'C-type-repr'


c = C()
c.__len__ = lambda: 99
c.__repr__ = lambda: 'instance-repr'
print(len(c), repr(c), c.__len__(), c.__repr__())
C.__len__ = lambda self: 2
print(len(c))
del C.__len__
try:
    len(c)
except TypeError as e:
    print(e)
C.__add__ = lambda self, o: f'added {o}'
print(c + 1)
C.__eq__ = lambda self, o: True
print(c == 5, C.__hash__ is not None)
try:
    hash(c)
except TypeError as e:
    print('still hashable via object?', e)
C.__iter__ = lambda self: iter([1, 2])
print(list(c), [x for x in c])
C.__getitem__ = lambda self, k: k * 2
print(c[21])
C.__call__ = lambda self, *a: a
print(c(1, 2), callable(c))
C.__bool__ = lambda self: False
print(bool(c), 'yes' if c else 'no')
C.__contains__ = lambda self, x: x == 'magic'
print('magic' in c, 'other' in c)


class Parent:
    def __str__(self):
        return 'parent-str'


class Kid(Parent):
    pass


k = Kid()
print(str(k))
Parent.__str__ = lambda self: 'patched parent-str'
print(str(k), f'{k}')
Kid.__str__ = lambda self: 'kid-str'
print(str(k))
del Kid.__str__
print(str(k))


class Ctx:
    pass


cm = Ctx()
cm.__enter__ = lambda: 1
cm.__exit__ = lambda *a: None
try:
    with cm:
        pass
except TypeError as e:
    print(e)
Ctx.__enter__ = lambda self: 'entered'
Ctx.__exit__ = lambda self, *a: None
with Ctx() as v:
    print(v)


class NextLater:
    def __iter__(self):
        return self


nl = NextLater()
try:
    next(nl)
except TypeError as e:
    print(e)
NextLater.__next__ = lambda self: 'nexted'
print(next(nl))


class HashLater:
    pass


h = HashLater()
d = {h: 1}
HashLater.__hash__ = lambda self: 42
# Mutating a key's hash violates dict's contract: lookup may find it by
# identity in the new probe sequence or miss it, depending on its old hash.
# Check the new special method and the stored entry without another lookup.
print(hash(h), next(iter(d)) is h, list(d.values()))

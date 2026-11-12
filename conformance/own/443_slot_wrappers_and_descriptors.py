# Builtin types' dunder methods called directly (the slot wrappers a
# subclass's super() reaches), list/set __init__ and __new__ as functions,
# object's own dunders, property.getter and descriptor type errors.
print([1].__eq__([1]), [1].__eq__((1,)), (1,).__lt__((2,)), (1).__eq__(True), (1.5).__ge__(1.5))
print({1: 2}.__eq__({1: 2}), b'a'.__gt__(b'b'), 'a'.__le__('b'), (2).__gt__(1), (1.0).__ne__(1))
print(list(iter([1, 2]).__iter__()), list({3: 4}.__iter__()), hash(()) == ().__hash__(), (1.5).__hash__() == hash(1.5),
      b'x'.__hash__() == hash(b'x'), 'x'.__hash__() == hash('x'), (7).__hash__())
print([1].__ne__([2]), (1,).__gt__((0,)), (1,).__ge__((1,)), (1,).__le__((0,)), [1].__lt__([1, 0]), [2].__ge__([1]))
print([1].__eq__(1), (1,).__lt__([1]), 'a'.__eq__(b'a'), (1).__lt__(1.5), {}.__ne__({}))

l = [1, 2]
list.__init__(l, 'ab')
print(l)
list.__init__(l)
print(l)
s = {1, 2}
set.__init__(s, [3, 4])
print(sorted(s))
set.__init__(s)
print(s)


class L(list):
    pass


print(list.__new__(L), type(list.__new__(L)).__name__, set.__new__(set))
for f in (lambda: list.__new__(int), lambda: list.append((), 1), lambda: str.upper(5), lambda: str.split(b'a'),
          lambda: dict.get([], 1), lambda: bytes.hex('a'), lambda: int.bit_length(1.5)):
    try:
        print(f())
    except TypeError as e:
        print(e)


class A:
    pass


a = A()
print(object.__eq__(a, a), object.__eq__(a, 1), object.__ne__(a, a), object.__ne__(a, 1))


class E:
    def __eq__(self, o):
        return 'eq-called'


print(object.__ne__(E(), 1))
print(object.__getattribute__(a, '__class__').__name__)
a.x = 3
print(object.__getattribute__(a, 'x'))
print(object.__repr__(a).startswith('<__main__.A object at 0x'), BaseException.__repr__(OSError()))


class Prop:
    p = property(lambda s: 1)
    p = p.getter(lambda s: 2)


print(Prop().p)


class G:
    def __getattr__(self, n):
        return n * 2


g = G()
print(g.xy, getattr(g, 'q'), hasattr(g, 'z'))

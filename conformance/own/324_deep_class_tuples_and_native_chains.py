# isinstance/issubclass accept tuples nested to any depth, tried left to
# right; a chain of builtins calling builtins is bounded like recursion.
t = (int,)
for _ in range(50000):
    t = (t,)
print(isinstance(1, t), isinstance('a', t), issubclass(bool, t), issubclass(str, t))
print(isinstance('a', (((int,),), (bytes, (str,)))))
print(isinstance(1, ((int,), 'not a type')))
for f, x in [(isinstance, 1), (issubclass, int)]:
    try:
        f(x, ((str,), 'not a type', int))
    except TypeError as e:
        print(type(e).__name__, e)


class Meta(type):
    def __instancecheck__(cls, obj):
        return obj == 'magic'


class M(metaclass=Meta):
    pass


print(isinstance('magic', (int, ((M,),))), isinstance('other', ((M,), str)))


class Foo:
    pass


Foo.__repr__ = Foo.__str__
try:
    repr(Foo())
except RecursionError:
    print('RecursionError')
print(repr(Foo).startswith("<class '"))

# The descriptor is unbound on type and its subclasses, and bound when
# found on the metaclass of a regular class.


class C:
    pass


class Meta(type):
    pass


class D(C, metaclass=Meta):
    pass


assert type.mro(C) == [C, object]
mro = type.mro
assert repr(mro) == "<method 'mro' of 'type' objects>"
assert mro is type.__dict__['mro'] is Meta.mro
assert mro(type) == [type, object]
assert mro(D) == [D, C, object]
assert int.mro.__self__ is int
assert int.mro() == [int, object]
assert C.mro.__self__ is C
assert C.mro() == [C, object]

for args in ((), (1,), (C, C)):
    try:
        mro(*args)
    except TypeError as e:
        print(e)
    else:
        assert False, 'expected TypeError'

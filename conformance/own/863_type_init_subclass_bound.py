class C:
    pass


Made = type.__new__(type, 'Made', (), {})
assert type(Made) is type and Made.__name__ == 'Made' and Made.__bases__ == (object,)


for cls in (type, object, int, C):
    hook = cls.__init_subclass__
    assert hook.__self__ is cls
    assert cls.__init_subclass__() is None
    try:
        hook(tag=True)
    except TypeError as e:
        assert str(e) == cls.__name__ + '.__init_subclass__() takes no keyword arguments'
    else:
        raise AssertionError('unexpected class keyword accepted')

assert C().__init_subclass__.__self__ is C

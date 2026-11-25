# Numeric slots use the first class definition in the MRO and bind it to
# the instance. Explicit None blocks an inherited method, not the slot.
class Ordinary:
    def __index__(self):
        return 1


class Inherited(Ordinary):
    pass


class Disabled(Ordinary):
    __index__ = None


class Static(Ordinary):
    @staticmethod
    def __index__():
        return 0


class IndexDescriptor:
    def __get__(self, obj, owner):
        assert obj is not None and owner is type(obj)
        return lambda: obj.number


class Described(Ordinary):
    __index__ = IndexDescriptor()
    number = -1


for obj in (Inherited(), Disabled(), Static(), Described()):
    for use in (bin, lambda x: [10, 20, 30][x]):
        try:
            print(use(obj))
        except TypeError as exc:
            print(type(exc).__name__, str(exc))


class ListIndex(list):
    __index__ = list.__len__


x = ListIndex([4, 5])
print(bin(x), [10, 20, 30][x], list(range(x)), 'abc'[:x])


class DisabledIter:
    __iter__ = None

    def __getitem__(self, key):
        raise AssertionError('must not try sequence iteration')


for use in (iter, tuple):
    try:
        use(DisabledIter())
    except TypeError as exc:
        print(type(exc).__name__, str(exc))

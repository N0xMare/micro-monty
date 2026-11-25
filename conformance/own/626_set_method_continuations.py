calls = []


class Key:
    def __init__(self, name):
        self.name = name

    def __hash__(self):
        return 0

    def __eq__(self, other):
        calls.append((self.name, other.name))
        return False


# Mutating methods return None after both a native park on a large set and
# a handoff to Python equality on a small set.
for kind in (set, frozenset):
    t = kind(range(2, 5002))
    for method, large_size, small_size in (
        ('update', 5002, 2),
        ('difference_update', 2, 1),
        ('intersection_update', 4998, 0),
        ('symmetric_difference_update', 4, 2),
    ):
        s = set(range(5000))
        assert getattr(s, method)(t) is None
        assert len(s) == large_size

        s = {Key('left')}
        assert getattr(s, method)(kind([Key('right')])) is None
        assert len(s) == small_size

# Toggling a missing key probes once to discard and once to add. The
# copy method starts with the source's keys, the mutating method with s.
for make_iter in (lambda x: (k for k in [x]), lambda x: map(lambda k: k, [x])):
    left, right = Key('left'), Key('right')
    s = {left}
    calls.clear()
    assert len(s.symmetric_difference(make_iter(right))) == 2
    assert calls == [('right', 'left'), ('right', 'left')]

    calls.clear()
    assert s.symmetric_difference_update(make_iter(right)) is None
    assert len(s) == 2
    assert calls == [('left', 'right'), ('left', 'right')]

# Descriptor ownership holds on both the direct and the variadic path.
for owner, receiver in ((set, frozenset({1})), (frozenset, {1})):
    for method in ('union', 'intersection', 'difference'):
        for args in (([2],), ([2], [3])):
            try:
                getattr(owner, method)(receiver, *args)
            except TypeError as error:
                assert str(error) == (
                    f"descriptor '{method}' for '{owner.__name__}' objects doesn't "
                    f"apply to a '{type(receiver).__name__}' object"
                )
            else:
                raise AssertionError(method)


class SetSubclass(set):
    pass


assert set.union(SetSubclass({1}), [2], [3]) == {1, 2, 3}

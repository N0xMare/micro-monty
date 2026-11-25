class Bad:
    def __hash__(self):
        raise ValueError('hash')


def later():
    raise AssertionError('a later source ran after the error')
    yield 0


# Mutating updates keep their successful prefix. Intersection update and
# the non-mutating methods leave the receiver alone.
for method, first, remaining in (
    ('update', [0, 1, 2], {0, 1, 2, 3, 4}),
    ('difference_update', [0], {2, 3}),
    ('intersection_update', [0, 1, 2], {0, 1, 2, 3}),
    ('union', [0], {0, 1, 2, 3}),
    ('difference', [0], {0, 1, 2, 3}),
    ('intersection', [0, 1, 2], {0, 1, 2, 3}),
):
    s = {0, 1, 2, 3}
    it = iter([1, 4, Bad(), 5])
    try:
        getattr(s, method)(first, (), (), it, later())
    except ValueError as error:
        assert str(error) == 'hash'
    else:
        raise AssertionError(method)
    assert s == remaining
    assert list(it) == [5]

s = frozenset({0, 1, 2, 3})
assert s.difference({0}, iter([1]), [2]) == frozenset({3})
assert type(s.difference({0}, [1])) is frozenset
assert type(s.union([4], [5])) is frozenset
assert type(s.intersection([0, 1], [1, 2])) is frozenset

# Intersection stops at its last match, even when inserting that match
# grows its temporary table and more arguments remain.
it = iter([5, 4, 3, 2, 1, Bad(), 8])
s = set(range(8))
s.intersection_update([1, 2, 3, 4, 5], it, [2])
assert s == {2}
assert type(next(it)) is Bad
assert next(it) == 8

# Symmetric difference takes only one source. A rejected call must not
# take anything from its iterator; a failing valid call keeps the tail.
for method in ('symmetric_difference', 'symmetric_difference_update'):
    s = {1, 2}
    it = iter([2, Bad(), 5])
    try:
        getattr(s, method)(it, [])
    except TypeError:
        pass
    else:
        raise AssertionError(method)
    assert next(it) == 2
    try:
        getattr(s, method)(it)
    except ValueError as error:
        assert str(error) == 'hash'
    else:
        raise AssertionError(method)
    assert s == {1, 2}
    assert list(it) == [5]

calls = []


class Key:
    def __init__(self, name, value):
        self.name = name
        self.value = value

    def __hash__(self):
        calls.append(('hash', self.name))
        return 0

    def __eq__(self, other):
        calls.append((self.name, other.name))
        return self.value == other.value


class SetSubclass(set):
    pass


# The cutoff uses a shift, not multiplication: with one element on the
# left, 15 elements on the right delete directly, but 16 intersect first.
for n, expected in (
    (15, [('left', 'miss'), ('left', 'hit')]),
    (16, [('miss', 'left'), ('hit', 'left')]),
):
    source = {Key('miss', 2), Key('hit', 3)}
    source.update(range(100, 100 + n - 2))
    big = frozenset(source)
    before = list(big)
    for cls in (set, SetSubclass):
        for method in (False, True):
            s = cls([Key('left', 3)])
            alias = s
            calls.clear()
            if method:
                assert s.difference_update(big) is None
            else:
                s -= big
            assert calls == expected
            assert s is alias and not s
            assert all(a is b for a, b in zip(before, big))

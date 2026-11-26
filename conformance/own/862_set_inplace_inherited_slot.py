def wrong(*args):
    raise AssertionError('binary override called instead of inherited in-place slot')


class S(set):
    __sub__ = __and__ = __or__ = __xor__ = wrong
    __rsub__ = __rand__ = __ror__ = __rxor__ = wrong
    __iter__ = __len__ = wrong


for cls in (S, set):
    rhs = S(range(2, 500))
    s = original = cls((1, 2))
    s -= rhs
    assert s is original and set.__eq__(s, {1})
    s = original = cls((1, 2))
    s &= rhs
    assert s is original and set.__eq__(s, {2})
    s = original = cls((1, 2))
    s |= rhs
    assert s is original and set.__eq__(s, set(range(1, 500)))
    s = original = cls((1, 2))
    s ^= rhs
    assert s is original and set.__eq__(s, {1}.union(range(3, 500)))


class Key:
    def __init__(self, label):
        self.label = label

    def __hash__(self):
        return 0

    def __eq__(self, other):
        seen.append(self.label)
        return isinstance(other, Key)


# Subtracting a much larger set finds the intersection first, so the
# right-hand key compares itself to the left instead of the reverse.
seen = []
s = original = S([Key('left')])
rhs = S([Key('right')])
rhs.update(range(1, 25))
s -= rhs
assert s is original and set.__eq__(s, set()) and seen == ['right']


class Own(S):
    def __isub__(self, other):
        assert self is original
        return 'own in-place override'


s = original = Own((1, 2))
s -= rhs
assert s == 'own in-place override' and set.__eq__(original, {1, 2})


class Declines(set):
    def __isub__(self, other):
        return NotImplemented

    def __sub__(self, other):
        return 'binary fallback'


s = original = Declines((1, 2))
s -= rhs
assert s == 'binary fallback' and original == {1, 2}


class DistinctKey(Key):
    __hash__ = Key.__hash__

    def __eq__(self, other):
        seen.append(self.label)
        return False


# A direct set comparison probes its table even when both operands are
# the same. Comparing the same set as a container element still skips it.
for cls in (set, S, frozenset):
    s = cls([DistinctKey('first'), DistinctKey('second')])
    seen.clear()
    assert s == s and seen == ['first']
    seen.clear()
    assert not (s != s) and seen == ['first']
    seen.clear()
    assert [s] == [s] and seen == []
